//! The first half of the Stage 2 gate, without the network: a match run from its session log and
//! the replay of that log in a bare `World` agree on the state hash after every tick, and so does
//! the replay of the log's file.

use std::fmt::Write;
use std::fs;
use std::process::Command;

use campfire_capabilities::{Action, Destination, Health, Order};
use campfire_math::{Num, Vec3};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{
    Applied, Delegation, DelegationTerms, InputChain, PlayerSlot, SeedError, ServerSeed,
    SessionHeader, SessionId, SessionLog, SessionPlayer,
};
use campfire_runner::Runner;
use campfire_sim::{EntityIndex, Position, StableId, StateHash};
use campfire_verifier::Replay;

const SERVER_SEED: ServerSeed = ServerSeed::new([9; 32]);
const SESSION_ID: SessionId = SessionId::new([7; 32]);
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

/// The player's main key, `key(1)`, lets `session_key` sign in this session.
fn delegation() -> Delegation {
    let terms = DelegationTerms {
        session_key: session_key().x_only_public_key().0,
        server_key: SERVER_KEY,
        session_id: SESSION_ID,
        expiration: 1_700_086_400,
    };
    Delegation::sign(&Secp256k1::new(), &key(1), &terms, 1_700_000_000, &AUX)
}

fn header() -> SessionHeader {
    SessionHeader {
        session_id: SESSION_ID,
        server_key: SERVER_KEY,
        max_input_delay: 3,
        max_input_lead: 3,
        max_payload_len: 64,
        max_inputs_per_tick: 4,
        seed_commitment: SERVER_SEED.commitment(),
        players: vec![SessionPlayer {
            delegation: delegation(),
            seed_contribution: [4; 32],
        }],
    }
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
    let mut chain = InputChain::new(PlayerSlot::new(0), delegation().chain_root());
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
            let signature = chain.sign(&secp, &session_key(), SESSION_ID, &AUX);
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

/// The stable ids of the units in `runner`'s world, and each one's health, rounded.
fn units(runner: &Runner) -> Vec<(u64, i64)> {
    let world = runner.world();
    world
        .resource::<EntityIndex>()
        .iter()
        .map(|(id, entity)| {
            let health = world.entity(entity).get::<Health>().unwrap();
            (id.get(), health.current().round())
        })
        .collect()
}

#[test]
fn towers_kill_creeps_and_the_replay_agrees() {
    // Ids: the hero 0, the first side's tower 1 at x = −8 and the second's 2 at x = 8, then
    // tick 0's wave: the first side's creeps 3 and 4 at x = −16, the second's 5 and 6 at 16.
    // A creep walks ⅛ m a tick from tick 1: the first side's is at −16 + t/8 after tick t.
    // The second side's tower sees it within 7.75 m once 24 − (t − 1)/8 ≤ 7.75, in tick 131,
    // and takes creep 3, the lower id of the two tied. Attacks start in ticks 131, 168 and
    // 205, every 37, and strike 5 ticks later for 150: 445 → 295 → 145 → 0 in tick 210. The
    // second side's creep 5 mirrors it.
    let before = run(&ORDERS.each_ref(), 210);
    assert_eq!(
        units(&before.runner),
        [
            (0, 600),
            (1, 1500),
            (2, 1500),
            (3, 145),
            (4, 445),
            (5, 145),
            (6, 445)
        ]
    );
    let Run { runner, hashes } = run(&ORDERS.each_ref(), 211);
    assert_eq!(
        units(&runner),
        [(0, 600), (1, 1500), (2, 1500), (4, 445), (6, 445)]
    );

    let decoded = SessionLog::decode(&encoded(runner.log())).unwrap();
    let mut replay = Replay::new(decoded).unwrap();
    for (tick, live) in hashes.iter().enumerate() {
        assert!(replay.run_tick());
        assert_eq!(replay.runner().state_hash(), *live, "tick {tick}");
    }
    assert!(!replay.run_tick());
    assert_eq!(units(replay.runner()), units(&runner));
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
    // What no signature, chain link or commitment covers: the max delay and lead (3 ^ 1 = 2), the
    // max payload length (64 ^ 1 = 65), the max inputs per tick (4 ^ 1 = 5), and the contribution
    // under every flip. The session key signs every order, the last one too.
    assert_eq!(replays, 4 + 32 * 3);
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
fn the_seed_comes_only_from_the_header_and_its_reveal() {
    assert_eq!(Replay::new(log()).err(), Some(SeedError::NotRevealed));
    assert_eq!(
        Runner::new(log(), ServerSeed::new([8; 32])).err(),
        Some(SeedError::WrongSeed)
    );
}
