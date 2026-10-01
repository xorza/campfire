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
use campfire_math::{Num, PlayerSlot, Vec3};
use campfire_package::{ModePackages, PackageStore, RELEASE, StoreError};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{
    Applied, Delegation, DelegationTerms, Fingerprint, InputChain, SeedChain, SeedError,
    ServerSeed, SessionHeader, SessionLog, SessionTerms,
};
use campfire_runner::{Runner, Session, StartError};
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
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
const SERVER_KEY: [u8; 32] = [8; 32];
/// BIP-340 signing without auxiliary randomness is deterministic, so every run signs alike.
const AUX: [u8; 32] = [0; 32];
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
        applied: Applied::At(0),
    },
    // Stamped 2 ticks ahead; 2 m along −x: ticks 22 to 29.
    Sent {
        arrives: 20,
        stamp: 22,
        x: -2,
        z: 5,
        applied: Applied::At(22),
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

fn key(byte: u8) -> Keypair {
    Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_byte_array(&[byte; 32]).unwrap(),
    )
}

fn session_key() -> Keypair {
    key(2)
}

fn packages() -> ModePackages {
    ModePackages::from_dir(Path::new(LANE_MODE)).unwrap()
}

fn store() -> PackageStore {
    PackageStore::scan(Path::new(PACKAGES)).unwrap()
}

/// A session of the test lane mode at its 30 ticks a second.
fn terms() -> SessionTerms {
    let packages = packages();
    SessionTerms {
        server_key: SERVER_KEY,
        tick_hz: NonZeroU32::new(30).unwrap(),
        max_input_delay: 3,
        max_input_lead: 3,
        max_payload_len: 64,
        max_inputs_per_tick: 4,
        seed_commitment: SEED_CHAIN.commitment(),
        release: RELEASE.to_owned(),
        mode: Session::mode_in_terms(&packages),
        dependencies: Session::dependencies_in_terms(&packages),
    }
}

/// The player's main key, `key(1)`, lets `session_key` sign in the session of `session`.
fn delegation(session: &SessionTerms) -> Delegation {
    let terms = DelegationTerms {
        session_key: session_key().x_only_public_key().0,
        server_key: SERVER_KEY,
        session_id: session.session_id(),
        seed_contribution: [4; 32],
        expiration: 1_700_086_400,
    };
    Delegation::sign(&Secp256k1::new(), &key(1), &terms, 1_700_000_000, &AUX)
}

/// The header of a session of `terms`, with the one player.
fn header_of(terms: SessionTerms) -> SessionHeader {
    SessionHeader {
        players: vec![delegation(&terms)],
        terms,
    }
}

fn header() -> SessionHeader {
    header_of(terms())
}

fn log() -> SessionLog {
    SessionLog::new(header()).unwrap()
}

fn num(value: i64) -> Num {
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
    let mut runner = Runner::new(log(), SEED_CHAIN.seed(0), &packages()).unwrap();
    let secp = Secp256k1::new();
    let mut chain = InputChain::new(PlayerSlot::new(0), delegation(&terms()).chain_root());
    let mut applied = Vec::new();
    let mut hashes = Vec::new();
    for tick in 0..ticks {
        for sent in orders.iter().filter(|sent| sent.arrives == tick) {
            let hero = hero_id(&runner);
            let payload = Order::payload(&[Order {
                unit: hero,
                action: Action::Move {
                    x: num(sent.x),
                    z: num(sent.z),
                },
            }]);
            let input = chain.extend(sent.stamp, &payload);
            let signature = chain.sign(&secp, &session_key(), terms().session_id(), &AUX);
            assert_eq!(
                runner.record([input], &signature, &mut applied),
                Ok(()),
                "{sent:?}"
            );
            assert_eq!(applied, [sent.applied], "{sent:?}");
        }
        runner.run_tick();
        hashes.push(runner.state_hash());
    }
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
    applied: Applied::At(0),
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
    // takes a hero within 7.75 m of its center: in tick 1 the hero at 0.25 is √(7.75² + 1) ≈ 7.81
    // m away, beyond; in tick 9, at 1, √(7² + 1) ≈ 7.07 m, within: no creep is in reach, so it
    // takes the hero. Its reach from edge to edge is 7.75 + 0.9 + 0.5 m from its center, so its
    // attacks start in ticks 9 and 46 and fire 5 ticks later, from (8, −3), at the hero 7.07 m
    // away. A projectile flies 12 m/s ÷ 30, 0.4 m rounded down to 6710886 / 2²⁴ m, from the tick
    // after it fires: 17 steps leave less than a step, so the 18th lands, in ticks 32 and 69, for
    // 150 each.
    //
    // The east creeps walk ⅛ m a tick from tick 1, where their bodies, both at 15.875, part to
    // 15.525 and 16.225; each thinks in tick t where the tick before left it. Creep 5 thinks in
    // tick 69 at 15.525 − 67/8 ≈ 7.15, √(6.15² + 2²) ≈ 6.47 m from the hero, within its 7 m aggro
    // range, and in tick 61 at 8.15, √(7.15² + 4) ≈ 7.42 m, beyond: it takes the hero in tick
    // 69. Creep 6 thinks in tick 70 at 16.225 − 68/8 = 7.725, √(6.725² + 4) ≈ 7.02 m, just beyond:
    // it takes no one. The west creeps are 15 m from the east's, and tower 0 is 15.5 m from
    // them: none of them takes a target.
    let expected: Vec<_> = (0..72)
        .map(|tick| {
            let hero = Some(2);
            let from = |first: u64| (tick >= first).then_some(()).and(hero);
            let hero_health = match tick {
                ..32 => 600,
                32..69 => 450,
                _ => 300,
            };
            let projectiles = usize::from((14..32).contains(&tick) || (51..69).contains(&tick));
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
    assert!(matches!(
        Replay::new(log(), &store),
        Err(StartError::Seed(SeedError::NotRevealed))
    ));
    assert!(matches!(
        Runner::new(log(), ServerSeed::new([8; 32]), &packages()),
        Err(StartError::Seed(SeedError::WrongSeed))
    ));

    // Each change to the terms, the log revealed, and the error the verifier refuses it with.
    let other = |change: fn(&mut SessionTerms)| {
        let mut terms = terms();
        change(&mut terms);
        let mut log = SessionLog::new(header_of(terms)).unwrap();
        log.reveal_seed(SEED_CHAIN.seed(0));
        Replay::new(log, &store).err()
    };
    let refused = [
        other(|terms| terms.release = "0.0.9".to_owned()),
        other(|terms| terms.mode = Fingerprint::new([0; 32])),
        other(|terms| terms.dependencies[0] = Fingerprint::new([0; 32])),
        other(|terms| terms.dependencies.clear()),
        other(|terms| terms.tick_hz = NonZeroU32::new(60).unwrap()),
    ];
    assert!(
        matches!(
            &refused,
            [
                Some(StartError::OtherRelease(release)),
                Some(StartError::Packages(StoreError::UnknownMode)),
                Some(StartError::Packages(StoreError::MissingDependency(dependency))),
                Some(StartError::Packages(StoreError::DependencyCount)),
                Some(StartError::TickRate(hz)),
            ] if release == "0.0.9" && dependency == "hero-runner" && hz.get() == 60
        ),
        "{refused:?}"
    );
}
