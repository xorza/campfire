//! The first half of the Stage 2 gate, without the network: a match of the test lane mode run from
//! its session log and the replay of that log in a bare `World` agree on the state hash after
//! every tick, and so does the replay of the log's file with the packages a verifier holds.

use std::fs;
use std::num::NonZeroU32;
use std::path::Path;
use std::process::Command;

use campfire_capabilities::{
    Action, ActionSlots, Destination, Order, Owner, PoolId, Pools, Projectile,
};
use campfire_log::LogEvent;
use campfire_math::{Num, Tick, Ticks, Vec3};
use campfire_package::{ModePackages, PackageStore, StoreError};
use campfire_protocol::{Applied, Fingerprint, SeedError, ServerSeed, SessionLog, SessionTerms};
use campfire_runner::{FixedSession, InputRules, Runner, StartError, TermsError};
use campfire_sim::{EntityIndex, Position, StableId, StateHash};
use campfire_verifier::{Replay, Verified};

/// Every package, the reference ones and the test ones: what the verifier holds.
/// The lane mode's life pool, `health`, the first of its pools by name.
const LIFE: PoolId = PoolId::FIRST;

const PACKAGES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages");
const LANE_MODE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/test/modes/lane"
);
/// Long enough for every order to apply, and for the hero to arrive.
const TICKS: u64 = 40;

/// A move order as the player sends it.
#[derive(Debug)]
struct Sent {
    /// The tick it arrives before.
    arrives: u64,
    stamp: u64,
    x: i64,
    z: i64,
    applied: Applied,
}

/// At a quarter meter a tick, straight lines take exactly 4 ticks a meter.
const ORDERS: [Sent; 3] = [
    // 5 m along z: ticks 0 to 19.
    Sent {
        arrives: 0,
        stamp: 0,
        x: 0,
        z: 5,
        applied: Applied::At(Tick::new(0)),
    },
    // Stamped 2 ticks ahead; 2 m along −x: ticks 22 to 29.
    Sent {
        arrives: 20,
        stamp: 22,
        x: -2,
        z: 5,
        applied: Applied::At(Tick::new(22)),
    },
    // 4 ticks after its stamp, past the max delay of 3: logged, never applied.
    Sent {
        arrives: 30,
        stamp: 26,
        x: 9,
        z: 9,
        applied: Applied::Late,
    },
];

fn packages() -> ModePackages {
    ModePackages::from_dir(Path::new(LANE_MODE)).unwrap()
}

fn store() -> PackageStore {
    PackageStore::scan(Path::new(PACKAGES)).unwrap()
}

/// A session of the test lane mode at its 30 ticks a second, for its one player, whose inputs
/// may come 3 ticks late or early.
fn session() -> FixedSession {
    let rules = InputRules {
        max_input_delay: Ticks::new(3),
        max_input_lead: Ticks::new(3),
        max_payload_len: 64,
        max_inputs_per_tick: 4,
    };
    FixedSession::with_rules(packages(), NonZeroU32::new(30).unwrap(), 1, rules)
}

const fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

#[derive(Debug, PartialEq, Eq)]
struct Hero {
    position: Position,
    destination: Destination,
}

/// The player's hero: the one unit under their control.
fn hero_id(runner: &Runner) -> StableId {
    let world = runner.world();
    let mut units = world.resource::<EntityIndex>().iter();
    let hero = units.find(|&(_, entity)| world.entity(entity).contains::<Owner>());
    hero.unwrap().0
}

fn hero(runner: &Runner) -> Hero {
    let world = runner.world();
    let entity = world
        .resource::<EntityIndex>()
        .get(hero_id(runner))
        .unwrap();
    let hero = world.entity(entity);
    Hero {
        position: *hero.get::<Position>().unwrap(),
        destination: *hero.get::<Destination>().unwrap(),
    }
}

#[derive(Debug)]
struct Run {
    runner: Runner,
    /// The state hash after each tick.
    hashes: Vec<StateHash>,
}

/// Runs a match in which the player sends `orders`.
fn run(orders: &[&Sent], ticks: u64) -> Run {
    let mut fixed = session().start();
    let mut hashes = Vec::new();
    for tick in 0..ticks {
        for sent in orders.iter().filter(|sent| sent.arrives == tick) {
            let hero = hero_id(fixed.runner());
            let payload = Order::payload(&[Order {
                unit: hero,
                action: Action::Move {
                    x: num(sent.x),
                    z: num(sent.z),
                },
            }]);
            assert_eq!(
                fixed.send(0, Tick::new(sent.stamp), &payload),
                sent.applied,
                "{sent:?}"
            );
        }
        fixed.runner_mut().run_tick();
        hashes.push(fixed.runner().state_hash());
    }
    let mut runner = fixed.into_runner();
    runner.reveal_seed();
    Run { runner, hashes }
}

#[test]
fn run_and_replay_agree_on_every_tick() {
    let Run {
        runner,
        hashes: live,
    } = run(&ORDERS.each_ref(), TICKS);
    let arrived = Hero {
        position: Position::new(Vec3::new(num(-2), Num::ZERO, num(5))).unwrap(),
        destination: Destination::default(),
    };
    assert_eq!(hero(&runner), arrived);

    let decoded = SessionLog::decode(&encoded(runner.log())).unwrap();
    let mut replay = Replay::new(decoded, &store()).unwrap();
    let mut replayed = Vec::new();
    while replay.run_tick() {
        replayed.push(replay.runner().state_hash());
    }
    assert_eq!(replayed.len(), live.len());
    for (tick, (replayed, live)) in replayed.iter().zip(&live).enumerate() {
        assert_eq!(replayed, live, "tick {tick}");
    }
    assert_eq!(hero(replay.runner()), arrived);

    // Without the second order the hashes agree until it would apply, at tick 22, and differ
    // from then on: the hash sees the hero move.
    let without = run(&[&ORDERS[0], &ORDERS[2]], TICKS).hashes;
    let first_difference = live.iter().zip(&without).position(|(a, b)| a != b);
    assert_eq!(first_difference, Some(22));
    assert!(live[22..].iter().zip(&without[22..]).all(|(a, b)| a != b));
}

/// The hero walks 1 m along x from its spawn, (0, −2), into the reach of the east tower.
const INTO_REACH: Sent = Sent {
    arrives: 0,
    stamp: 0,
    x: 1,
    z: -2,
    applied: Applied::At(Tick::new(0)),
};

/// What a tick left: the attack target of the towers, units 0 and 1, and of the creeps, 3 to 6;
/// the hero's health; and how many projectiles fly.
#[derive(Debug, PartialEq, Eq)]
struct Seen {
    targets: [Option<u64>; 6],
    hero_health: i64,
    projectiles: usize,
}

impl Seen {
    fn of(runner: &Runner) -> Seen {
        let world = runner.world();
        let index = world.resource::<EntityIndex>();
        let entity = |id: u64| index.iter().find(|(unit, _)| unit.get() == id).unwrap().1;
        let targets = [0, 1, 3, 4, 5, 6].map(|id| {
            let slots = world.entity(entity(id)).get::<ActionSlots>().unwrap();
            slots.attack_target().map(StableId::get)
        });
        let hero = world.entity(entity(2)).get::<Pools>().unwrap();
        let projectiles = index
            .iter()
            .filter(|&(_, entity)| world.entity(entity).contains::<Projectile>())
            .count();
        Seen {
            targets,
            hero_health: hero.current(LIFE).unwrap().round(),
            projectiles,
        }
    }
}

#[test]
fn scripted_creeps_and_towers_replay_to_the_same_hashes() {
    let Run { runner, hashes } = run(&[&INTO_REACH], 72);
    let decoded = SessionLog::decode(&encoded(runner.log())).unwrap();
    let mut replay = Replay::new(decoded, &store()).unwrap();
    let mut seen = Vec::new();
    for (tick, live) in hashes.iter().enumerate() {
        assert!(replay.run_tick());
        assert_eq!(replay.runner().state_hash(), *live, "tick {tick}");
        seen.push(Seen::of(replay.runner()));
    }
    assert!(!replay.run_tick());

    // Ids: the map's towers, 0 at (−8, −3) on the west team and 1 at (8, −3) on the east, then the
    // hero 2, which the mode spawns as the match starts, then the first wave, at the end of tick
    // 0: the west creeps 3 and 4 at x = −16, and the east's 5 and 6 at x = 16. Every unit thinks
    // every 8 ticks, in the ticks that leave its id. Bodies: the towers 0.9 m, the hero 0.5 m, a
    // creep 0.35 m; a range counts from the edge of each body.
    //
    // The hero walks ¼ m a tick from (0, −2) in tick 0 and stands at (1, −2) from tick 3. Tower 1
    // takes the nearest hero that 6.25 m from the edge of its body reaches, 6.25 + 0.9 + 0.5 =
    // 7.65 m between centres: in tick 1 the hero at 0.25 is √(7.75² + 1) ≈ 7.81 m away, beyond; in
    // tick 9, at 1, √(7² + 1) ≈ 7.07 m, within: no creep is in reach, so it takes the hero. Its
    // weapon's reach is the same 7.65 m, so its attacks start in ticks 9 and 46, and fire 5 ticks
    // later, from (8, −3), at the hero 7.07 m away. A projectile of no width flies 12 m/s ÷ 30,
    // 0.4 m rounded down to 6710886 / 2²⁴ m, from the tick after it fires, and hits when it comes
    // within the hero's 0.5 m: 16 steps leave 7.07 − 6.4 ≈ 0.67 m, 17 leave 0.27 m, so the 17th
    // hits, in ticks 31 and 68, for 150 each.
    //
    // The east creeps walk ⅛ m a tick from tick 1, where their bodies, both at 15.875, part to
    // 15.525 and 16.225; each thinks in tick t where the tick before left it, and takes the hero
    // that its 6 m aggro range reaches from its edge, 6 + 0.35 + 0.5 = 6.85 m between centres.
    // Creep 5 thinks in tick 69 at 15.525 − 67/8 ≈ 7.15, √(6.15² + 2²) ≈ 6.47 m from the hero,
    // within, and in tick 61 at 8.15, √(7.15² + 4) ≈ 7.42 m, beyond: it takes the hero in tick 69.
    // Creep 6 thinks in tick 70 at 16.225 − 68/8 = 7.725, √(6.725² + 4) ≈ 7.02 m, beyond: it takes
    // no one. The west creeps are 15 m from the east's, and tower 0 is 15.5 m from them: none of
    // them takes a target.
    let expected: Vec<_> = (0..72)
        .map(|tick| {
            let hero = Some(2);
            let from = |first: u64| (tick >= first).then_some(()).and(hero);
            let hero_health = match tick {
                ..31 => 600,
                31..68 => 450,
                _ => 300,
            };
            let projectiles = usize::from((14..31).contains(&tick) || (51..68).contains(&tick));
            Seen {
                targets: [None, from(9), None, None, from(69), None],
                hero_health,
                projectiles,
            }
        })
        .collect();
    assert_eq!(seen, expected);
}

fn encoded(log: &SessionLog) -> Vec<u8> {
    let mut bytes = Vec::new();
    log.encode(&mut bytes);
    bytes
}

#[test]
fn every_corruption_of_a_log_file_is_refused() {
    let bytes = encoded(run(&ORDERS.each_ref(), TICKS).runner.log());
    for len in 0..bytes.len() {
        assert!(
            SessionLog::decode(&bytes[..len]).is_err(),
            "truncated to {len} bytes"
        );
    }
    // A signature, a chain link or the commitment covers every byte: the session id hashes the
    // terms, which the delegation and every order sign, and the main key signs the contribution
    // in the delegation.
    for at in 0..bytes.len() {
        for flip in [0x01, 0x80, 0xFF] {
            let mut corrupt = bytes.clone();
            corrupt[at] ^= flip;
            assert!(
                SessionLog::decode(&corrupt).is_err(),
                "byte {at} ^ {flip:#x}"
            );
        }
    }
}

#[test]
fn the_binary_logs_the_last_state_hash() {
    let Run { runner, hashes } = run(&ORDERS.each_ref(), TICKS);
    let dir = env!("CARGO_TARGET_TMPDIR");
    let path = format!("{dir}/headless.log");
    fs::write(&path, encoded(runner.log())).unwrap();
    // Logged without color, as standard error is not a terminal here, and at `info`, whatever the
    // environment says.
    let verifier = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_campfire-verifier"))
            .args(args)
            .env("RUST_LOG", "info")
            .output()
            .unwrap()
    };
    let output = verifier(&[PACKAGES, &path]);
    assert!(output.status.success(), "{output:?}");
    let hash = hashes.last().unwrap();
    let logged = String::from_utf8(output.stderr).unwrap();
    let success = format!("{} file={path} hash={hash}\n", Verified::MESSAGE);
    assert!(logged.ends_with(&success), "{logged}");
    assert!(output.stdout.is_empty());

    let corrupt = format!("{dir}/headless-truncated.log");
    let bytes = encoded(runner.log());
    fs::write(&corrupt, &bytes[..bytes.len() - 1]).unwrap();
    let output = verifier(&[PACKAGES, &corrupt]);
    assert_eq!(output.status.code(), Some(1));
    let logged = String::from_utf8(output.stderr).unwrap();
    let refused =
        format!("the log does not verify file={corrupt} error=session log ends inside a field\n");
    assert!(logged.ends_with(&refused), "{logged}");

    assert_eq!(verifier(&[]).status.code(), Some(2));
}

#[test]
fn a_log_replays_only_with_its_seed_its_release_and_its_packages() {
    let store = store();
    let session = session();
    assert!(matches!(
        Replay::new(session.log(), &store),
        Err(StartError::Seed(SeedError::NotRevealed))
    ));
    assert!(matches!(
        Runner::new(session.log(), ServerSeed::new([8; 32]), &packages()),
        Err(StartError::Seed(SeedError::WrongSeed))
    ));

    // Each change to the terms, the log revealed, and the error the verifier refuses it with.
    let other = |change: fn(&mut SessionTerms)| {
        let mut terms = session.terms().clone();
        change(&mut terms);
        let mut log = SessionLog::new(session.header(terms)).unwrap();
        log.reveal_seed(FixedSession::seed());
        Replay::new(log, &store).err()
    };
    let refused = [
        other(|terms| terms.release = "0.0.9".to_owned()),
        // Another release first: its packages need not read in this one.
        other(|terms| {
            terms.release = "0.0.9".to_owned();
            terms.mode = Fingerprint::new([0; 32]);
        }),
        other(|terms| terms.mode = Fingerprint::new([0; 32])),
        other(|terms| terms.dependencies[0] = Fingerprint::new([0; 32])),
        other(|terms| terms.dependencies.clear()),
        other(|terms| terms.tick_hz = NonZeroU32::new(60).unwrap()),
    ];
    assert!(
        matches!(
            &refused,
            [
                Some(StartError::Terms(TermsError::OtherRelease(release))),
                Some(StartError::Terms(TermsError::OtherRelease(_))),
                Some(StartError::Packages(StoreError::UnknownMode)),
                Some(StartError::Packages(StoreError::MissingDependency(dependency))),
                Some(StartError::Packages(StoreError::DependencyCount)),
                Some(StartError::Terms(TermsError::TickRate(hz))),
            ] if release == "0.0.9" && dependency == "hero-runner" && hz.get() == 60
        ),
        "{refused:?}"
    );

    // Three players, and the lane's two teams seat one each.
    let mut header = session.header(session.terms().clone());
    header.players = vec![header.players[0].clone(); 3];
    let mut log = SessionLog::new(header).unwrap();
    log.reveal_seed(FixedSession::seed());
    let refused = Replay::new(log, &store).err();
    assert!(
        matches!(
            refused,
            Some(StartError::Players {
                players: 3,
                slots: 2
            })
        ),
        "{refused:?}"
    );
}
