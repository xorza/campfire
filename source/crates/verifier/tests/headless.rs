//! The first half of the Stage 2 gate, without the network: a match run from its session log and
//! the replay of that log in a bare `World` agree on the state hash after every tick, and so does
//! the replay of the log's file.

use std::fmt::Write;
use std::fs;
use std::process::Command;

use campfire_kit_moba::{Destination, Order};
use campfire_math::{Num, Vec3};
use campfire_protocol::{
    Applied, InputChain, InputHash, PlayerSlot, SeedError, ServerSeed, SessionHeader, SessionLog,
    SessionPlayer,
};
use campfire_runner::Runner;
use campfire_sim::{EntityIndex, Position, StateHash};
use campfire_verifier::Replay;

const SERVER_SEED: ServerSeed = ServerSeed::new([9; 32]);
const ROOT: InputHash = InputHash::new([3; 32]);
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

fn header() -> SessionHeader {
    SessionHeader {
        max_input_delay: 3,
        max_input_lead: 3,
        seed_commitment: SERVER_SEED.commitment(),
        players: vec![SessionPlayer {
            chain_root: ROOT,
            seed_contribution: [4; 32],
        }],
    }
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
fn run(orders: &[&Sent]) -> Run {
    let mut runner = Runner::new(header(), SERVER_SEED).unwrap();
    let mut chain = InputChain::new(PlayerSlot::new(0), ROOT);
    let mut hashes = Vec::new();
    for tick in 0..TICKS {
        for sent in orders.iter().filter(|sent| sent.arrives == tick) {
            let payload = Order::Move {
                x: num(sent.x),
                z: num(sent.z),
            }
            .encode();
            let input = chain.extend(sent.stamp, &payload);
            assert_eq!(runner.record(input), Ok(sent.applied), "{sent:?}");
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
    } = run(&ORDERS.each_ref());
    let arrived = Hero {
        position: Position::new(Vec3::new(num(-2), Num::ZERO, num(5))).unwrap(),
        destination: Destination::default(),
    };
    assert_eq!(hero(&runner), arrived);

    let decoded = SessionLog::decode(&encoded(runner.log())).unwrap();
    for published in [runner.log(), &decoded] {
        let mut replay = Replay::new(published).unwrap();
        let mut replayed = Vec::new();
        while let Some(outcome) = replay.next_tick() {
            outcome.unwrap();
            replayed.push(replay.runner().state_hash());
        }
        assert_eq!(replayed.len(), live.len());
        for (tick, (replayed, live)) in replayed.iter().zip(&live).enumerate() {
            assert_eq!(replayed, live, "tick {tick}");
        }
        assert_eq!(hero(replay.runner()), arrived);
    }

    // Without the second order the hashes agree until it would apply, at tick 22, and differ
    // from then on: the hash sees the hero move.
    let without = run(&[&ORDERS[0], &ORDERS[2]]).hashes;
    let first_difference = live.iter().zip(&without).position(|(a, b)| a != b);
    assert_eq!(first_difference, Some(22));
    assert!(live[22..].iter().zip(&without[22..]).all(|(a, b)| a != b));
}

fn encoded(log: &SessionLog) -> Vec<u8> {
    let mut bytes = Vec::new();
    log.encode(&mut bytes);
    bytes
}

#[test]
fn a_corrupt_log_file_is_refused_or_replays() {
    let bytes = encoded(run(&ORDERS.each_ref()).runner.log());
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
                let mut replay = Replay::new(&log).unwrap();
                while let Some(outcome) = replay.next_tick() {
                    outcome.unwrap();
                }
                replays += 1;
            }
        }
    }
    // What no chain link or commitment covers: the max delay and lead (3 ^ 1 = 2), the
    // contribution under every flip, and in the last order, which no later input links to, its
    // stamp (26 ^ 1 = 27, now applied at tick 30) and every payload byte under every flip.
    let payload = Order::Move {
        x: num(ORDERS[2].x),
        z: num(ORDERS[2].z),
    }
    .encode();
    assert_eq!(replays, 2 + 32 * 3 + 1 + payload.len() * 3);
}

#[test]
fn the_binary_prints_the_last_state_hash() {
    let Run { runner, hashes } = run(&ORDERS.each_ref());
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
    let unpublished = SessionLog::new(header());
    assert_eq!(
        Replay::new(&unpublished).err(),
        Some(SeedError::NotRevealed)
    );
    assert_eq!(
        Runner::new(header(), ServerSeed::new([8; 32])).err(),
        Some(SeedError::WrongSeed)
    );
}
