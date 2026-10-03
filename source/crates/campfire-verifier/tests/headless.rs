//! The first half of the Stage 2 gate, without the network: a match of the test lane mode run from
//! its session log and the replay of that log in a bare `World` agree on the state hash after
//! every tick, and so does the replay of the log's file with the packages a verifier holds. So
//! does the reference 3v3's, whose players learn ranks.

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use campfire_capabilities::{
    Action, ActionSlots, Destination, Order, Owner, PoolId, Pools, Projectile,
};
use campfire_common::{Fingerprint, Tick, Ticks};
use campfire_log::LogEvent;
use campfire_log::internals::LogCheck;
use campfire_math::{Num, Vec3};
use campfire_package::{ModePackages, PackageDir, PackageStore, StoreError};
use campfire_protocol::{Applied, SeedError, ServerSeed, SessionLog, SessionTerms};
use campfire_runner::internals::{FixedMatch, FixedSession, HashTrail, MatchUnits, Reference3v3};
use campfire_runner::{InputRules, Runner, StartError, TermsError};
use campfire_sim::{EntityIndex, Position, StableId};
use campfire_verifier::{Replay, Verified};
use tempfile::TempDir;

/// The lane mode's life pool, `health`, the first of its pools by name.
const LIFE: PoolId = PoolId::FIRST;

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
    ModePackages::from_dir(&PackageDir::workspace("test/modes/lane")).unwrap()
}

/// Every file under `dir`, by its path from `dir`.
fn files_under(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut dirs = vec![PathBuf::new()];
    while let Some(at) = dirs.pop() {
        for entry in fs::read_dir(dir.join(&at)).unwrap() {
            let entry = entry.unwrap();
            let path = at.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                dirs.push(path);
            } else {
                files.insert(path, fs::read(entry.path()).unwrap());
            }
        }
    }
    files
}

fn store() -> PackageStore {
    PackageStore::scan(&PackageDir::workspace("")).unwrap()
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
    fixed: FixedMatch,
    /// The state after each tick.
    trail: HashTrail,
}

/// Runs a match in which the player sends `orders`.
fn run(orders: &[&Sent], ticks: u64) -> Run {
    let mut fixed = session().start();
    let mut trail = HashTrail::default();
    for tick in 0..ticks {
        for sent in orders.iter().filter(|sent| sent.arrives == tick) {
            let hero = hero_id(fixed.runner());
            let payload = Order::payload(&[Order {
                unit: hero,
                action: Action::Move {
                    x: Num::int(sent.x),
                    z: Num::int(sent.z),
                },
            }]);
            assert_eq!(
                fixed.send(0, Tick::new(sent.stamp), &payload),
                sent.applied,
                "{sent:?}"
            );
        }
        fixed.runner_mut().run_tick();
        trail.record(fixed.runner().world());
    }
    fixed.runner_mut().reveal_seed();
    Run { fixed, trail }
}

#[test]
fn run_and_replay_agree_on_every_tick() {
    let Run { fixed, trail: live } = run(&ORDERS.each_ref(), TICKS);
    let runner = fixed.runner();
    let arrived = Hero {
        position: Position::new(Vec3::new(Num::int(-2), Num::ZERO, Num::int(5))).unwrap(),
        destination: Destination::default(),
    };
    assert_eq!(hero(runner), arrived);

    let decoded = SessionLog::decode(&encoded(runner.log())).unwrap();
    let mut replay = Replay::new(decoded, &store()).unwrap();
    let mut replayed = HashTrail::default();
    while replay.run_tick() {
        replayed.record(replay.runner().world());
    }
    live.assert_same(&replayed);
    assert_eq!(hero(replay.runner()), arrived);

    // Without the second order the hashes agree until it would apply, at tick 22, and differ
    // from then on: the hash sees the hero move.
    let without = run(&[&ORDERS[0], &ORDERS[2]], TICKS).trail;
    let (live, without) = (live.totals(), without.totals());
    let first_difference = live.iter().zip(without).position(|(a, b)| a != b);
    assert_eq!(first_difference, Some(22));
    assert!(live[22..].iter().zip(&without[22..]).all(|(a, b)| a != b));
}

#[test]
fn a_3v3_log_with_learn_orders_verifies_from_the_store() {
    // The scripted 3v3 to tick 1900, Rime's learn, the last of its learn orders.
    let reference = Reference3v3::load();
    let mut fixed = reference.start();
    let mut live = HashTrail::default();
    for tick in 0..=1900 {
        Reference3v3::play_tick(&mut fixed, tick);
        live.record(fixed.runner().world());
    }
    fixed.runner_mut().reveal_seed();

    let decoded = SessionLog::decode(&encoded(fixed.runner().log())).unwrap();
    let mut replay = Replay::new(decoded, &store()).unwrap();
    let mut replayed = HashTrail::default();
    while replay.run_tick() {
        replayed.record(replay.runner().world());
    }
    live.assert_same(&replayed);
    // The replay learned what the match did: Cinder's and Veil's first basic ability, and Rime's
    // second.
    let world = replay.runner().world();
    let units = MatchUnits::of_world(world);
    let ranks = [(0, 0), (5, 0), (4, 1)].map(|(slot, ability)| {
        let entity = world
            .resource::<EntityIndex>()
            .get(units.hero(slot))
            .unwrap();
        let slots = world.get::<ActionSlots>(entity).unwrap();
        slots.slot(ability).unwrap().rank
    });
    assert_eq!(ranks, [1, 1, 1]);
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
    let Run { fixed, trail } = run(&[&INTO_REACH], 72);
    let decoded = SessionLog::decode(&encoded(fixed.runner().log())).unwrap();
    let mut replay = Replay::new(decoded, &store()).unwrap();
    let mut replayed = HashTrail::default();
    let mut seen = Vec::new();
    while replay.run_tick() {
        replayed.record(replay.runner().world());
        seen.push(Seen::of(replay.runner()));
    }
    trail.assert_same(&replayed);

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
fn the_binary_logs_the_last_state_hash() {
    let Run { fixed, trail } = run(&ORDERS.each_ref(), TICKS);
    let runner = fixed.runner();
    // A directory of this run's own, which goes when the test ends, passed or failed.
    let scratch = TempDir::new_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let dir = scratch.path().to_str().unwrap();
    let path = format!("{dir}/headless.log");
    fs::write(&path, encoded(runner.log())).unwrap();
    // Logged without color, as standard error is not a terminal here, at `info`, and to no file,
    // whatever the environment says.
    let verifier = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_campfire-verifier"))
            .args(args)
            .env("RUST_LOG", "info")
            .env_remove("CAMPFIRE_LOG")
            .env_remove("CAMPFIRE_LOG_FILTER")
            .output()
            .unwrap()
    };
    // Every package, the reference ones and the test ones: what the verifier holds.
    let packages = PackageDir::workspace("");
    let packages = packages.to_str().unwrap();
    let output = verifier(&[packages, &path]);
    assert!(output.status.success(), "{output:?}");
    let hash = trail.totals().last().unwrap();
    let logged = String::from_utf8(output.stderr).unwrap();
    let success = format!("{} file={path} hash={hash}\n", Verified::MESSAGE);
    assert!(logged.ends_with(&success), "{logged}");
    assert!(output.stdout.is_empty());

    let corrupt = format!("{dir}/headless-truncated.log");
    let bytes = encoded(runner.log());
    fs::write(&corrupt, &bytes[..bytes.len() - 1]).unwrap();
    let output = verifier(&[packages, &corrupt]);
    assert_eq!(output.status.code(), Some(1));
    let logged = String::from_utf8(output.stderr).unwrap();
    let refused =
        format!("the log does not verify file={corrupt} error=session log ends inside a field\n");
    assert!(logged.ends_with(&refused), "{logged}");

    assert_eq!(verifier(&[]).status.code(), Some(2));
}

#[test]
fn a_log_replays_only_with_its_seed_its_release_and_its_packages() {
    let _log = LogCheck::start();
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

    // A lane mode whose `on_match_start` throws: the session does not start.
    let mut files = files_under(&PackageDir::workspace("test"));
    let script = PathBuf::from("modes/lane/scripts/mode.rhai");
    let text = String::from_utf8(files[&script].clone()).unwrap();
    let start = "fn on_match_start(ctx) {\n";
    assert!(text.contains(start));
    let text = text.replacen(
        start,
        "fn on_match_start(ctx) {\n    throw \"no start\";\n",
        1,
    );
    files.insert(script, text.into_bytes());
    let dir = PackageDir::in_memory(Arc::new(files), "modes/lane");
    let thrown = FixedSession::new(
        ModePackages::from_package_dir(&dir).unwrap(),
        NonZeroU32::new(30).unwrap(),
        1,
    );
    let refused = Runner::new(thrown.log(), FixedSession::seed(), thrown.packages()).err();
    let Some(refused @ StartError::MatchStart(_)) = refused else {
        panic!("{refused:?}");
    };
    assert_eq!(
        refused.to_string(),
        r#"the mode's start failed: script call raised "no start""#
    );
}
