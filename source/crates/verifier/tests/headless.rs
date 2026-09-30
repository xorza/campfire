//! The first half of the Stage 2 gate, without the network: a match run from its session log and
//! the replay of that log in a bare `World` agree on the state hash after every tick, and so does
//! the replay of the log's file.

use std::fmt::Write;
use std::fs;
use std::num::NonZeroU32;
use std::process::Command;

use campfire_capabilities::{Action, AttackState, Destination, Health, Order, Projectile};
use campfire_math::{Num, Vec3};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{
    Applied, Delegation, DelegationTerms, InputChain, PlayerSlot, SeedError, ServerSeed,
    SessionHeader, SessionLog, SessionPlayer, SessionTerms,
};
use campfire_runner::{Runner, StandInMode, StartError};
use campfire_sim::{EntityIndex, Position, StableId, StateHash};
use campfire_verifier::Replay;

const SERVER_SEED: ServerSeed = ServerSeed::new([9; 32]);
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

fn terms() -> SessionTerms {
    SessionTerms {
        server_key: SERVER_KEY,
        tick_hz: StandInMode::TICK_HZ,
        max_input_delay: 3,
        max_input_lead: 3,
        max_payload_len: 64,
        max_inputs_per_tick: 4,
        seed_commitment: SERVER_SEED.commitment(),
    }
}

/// The player's main key, `key(1)`, lets `session_key` sign in the session of `session`.
fn delegation(session: &SessionTerms) -> Delegation {
    let terms = DelegationTerms {
        session_key: session_key().x_only_public_key().0,
        server_key: SERVER_KEY,
        session_id: session.session_id(),
        expiration: 1_700_086_400,
    };
    Delegation::sign(&Secp256k1::new(), &key(1), &terms, 1_700_000_000, &AUX)
}

/// The header of a session of `terms`, with the one player.
fn header_of(terms: SessionTerms) -> SessionHeader {
    SessionHeader {
        terms,
        players: vec![SessionPlayer {
            delegation: delegation(&terms),
            seed_contribution: [4; 32],
        }],
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

fn hero(runner: &Runner) -> Hero {
    let world = runner.world();
    let (_, entity) = world.resource::<EntityIndex>().iter().next().unwrap();
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
    let mut runner = Runner::new(log(), SERVER_SEED).unwrap();
    let secp = Secp256k1::new();
    let mut chain = InputChain::new(PlayerSlot::new(0), delegation(&terms()).chain_root());
    let mut applied = Vec::new();
    let mut hashes = Vec::new();
    for tick in 0..ticks {
        for sent in orders.iter().filter(|sent| sent.arrives == tick) {
            // The hero is the first unit spawned, stable id 0.
            let hero = world_ids(&runner)[0];
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
    let mut replay = Replay::new(decoded).unwrap();
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

fn world_ids(runner: &Runner) -> Vec<StableId> {
    runner
        .world()
        .resource::<EntityIndex>()
        .iter()
        .map(|(id, _)| id)
        .collect()
}

/// The hero walks 1 m along x from the origin, into the reach of the second side's tower.
const INTO_REACH: Sent = Sent {
    arrives: 0,
    stamp: 0,
    x: 1,
    z: 0,
    applied: Applied::At(0),
};

/// What a tick left: the attack target of units 1 to 6, the hero's health, and how many
/// projectiles fly.
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
        let targets = [1, 2, 3, 4, 5, 6].map(|id| {
            let attack = world.entity(entity(id)).get::<AttackState>().unwrap();
            attack.target().map(StableId::get)
        });
        let hero = world.entity(entity(0)).get::<Health>().unwrap();
        let projectiles = index
            .iter()
            .filter(|&(_, entity)| world.entity(entity).contains::<Projectile>())
            .count();
        Seen {
            targets,
            hero_health: hero.current().round(),
            projectiles,
        }
    }
}

#[test]
fn scripted_creeps_and_towers_replay_to_the_same_hashes() {
    let Run { runner, hashes } = run(&[&INTO_REACH], 72);
    let decoded = SessionLog::decode(&encoded(runner.log())).unwrap();
    let mut replay = Replay::new(decoded).unwrap();
    let mut seen = Vec::new();
    for (tick, live) in hashes.iter().enumerate() {
        assert!(replay.run_tick());
        assert_eq!(replay.runner().state_hash(), *live, "tick {tick}");
        seen.push(Seen::of(replay.runner()));
    }
    assert!(!replay.run_tick());

    // Ids: the hero 0, the towers 1 at x = −8 and 2 at x = 8, then the first side's creeps 3 and
    // 4 at x = −16 and the second's 5 and 6 at x = 16. Every unit thinks every 8 ticks, in the
    // ticks that leave its id.
    //
    // The hero walks ¼ m a tick from tick 0 and stands at x = 1 from tick 3. Tower 2 thinks in
    // tick 2, with the hero at 0.5, 7.5 m away and within its 7.75: no creep is in reach, so it
    // takes the hero. Its attacks start in ticks 2, 39 and 76 and fire 5 ticks later, from
    // x = 8, at the hero 7 m away. A projectile flies 12 m/s ÷ 30, 0.4 m rounded down to
    // 6710886 / 2²⁴ m, from the tick after it fires: 17 steps leave less than a step, so the
    // 18th lands, in ticks 25 and 62, for 150 each.
    //
    // A second-side creep stands at 16 − (t − 1)/8 as it thinks in tick t. Creep 5 thinks in
    // tick 61 at 8.5, 7.5 m from the hero, beyond its 7 m aggro range, and in tick 69 at 7.5,
    // 6.5 m away: it takes the hero. Creep 6 does in tick 70, at 7.375. The first side's creeps
    // are 15 m from the second's, and tower 1 is 15.5 m from them: none of them takes a target.
    let expected: Vec<_> = (0..72)
        .map(|tick| {
            let hero = Some(0);
            let from = |first: u64| (tick >= first).then_some(()).and(hero);
            let hero_health = match tick {
                ..25 => 600,
                25..62 => 450,
                _ => 300,
            };
            let projectiles = usize::from((7..25).contains(&tick) || (44..62).contains(&tick));
            Seen {
                targets: [None, from(2), None, None, from(69), from(70)],
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
fn a_corrupt_log_file_is_refused_or_replays() {
    let bytes = encoded(run(&ORDERS.each_ref(), TICKS).runner.log());
    for len in 0..bytes.len() {
        assert!(
            SessionLog::decode(&bytes[..len]).is_err(),
            "truncated to {len} bytes"
        );
    }
    let mut replays = 0;
    for at in 0..bytes.len() {
        for flip in [0x01, 0x80, 0xFF] {
            let mut corrupt = bytes.clone();
            corrupt[at] ^= flip;
            if let Ok(log) = SessionLog::decode(&corrupt) {
                let mut replay = Replay::new(log).unwrap();
                while replay.run_tick() {}
                replays += 1;
            }
        }
    }
    // What no signature, chain link or commitment covers: the contribution, under every flip.
    // The session id hashes the terms, which the delegation and every order sign.
    assert_eq!(replays, 32 * 3);
}

#[test]
fn the_binary_prints_the_last_state_hash() {
    let Run { runner, hashes } = run(&ORDERS.each_ref(), TICKS);
    let dir = env!("CARGO_TARGET_TMPDIR");
    let path = format!("{dir}/headless.log");
    fs::write(&path, encoded(runner.log())).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_campfire-verifier"))
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut expected = String::new();
    for byte in hashes.last().unwrap().as_bytes() {
        write!(expected, "{byte:02x}").unwrap();
    }
    expected.push('\n');
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);

    let corrupt = format!("{dir}/headless-truncated.log");
    let bytes = encoded(runner.log());
    fs::write(&corrupt, &bytes[..bytes.len() - 1]).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_campfire-verifier"))
        .arg(&corrupt)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!("{corrupt}: session log ends inside a field\n")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_campfire-verifier"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_match_starts_only_with_its_seed_and_at_a_rate_its_mode_runs_at() {
    assert_eq!(
        Replay::new(log()).err(),
        Some(StartError::Seed(SeedError::NotRevealed))
    );
    assert_eq!(
        Runner::new(log(), ServerSeed::new([8; 32])).err(),
        Some(StartError::Seed(SeedError::WrongSeed))
    );
    let sixty = NonZeroU32::new(60).unwrap();
    let fast = SessionTerms {
        tick_hz: sixty,
        ..terms()
    };
    let log = SessionLog::new(header_of(fast)).unwrap();
    assert_eq!(
        Runner::new(log, SERVER_SEED).err(),
        Some(StartError::TickRate(sixty))
    );
}
