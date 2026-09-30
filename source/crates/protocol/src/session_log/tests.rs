use super::*;

const MAX_DELAY: u64 = 2;
const MAX_LEAD: u64 = 2;
const ROOTS: [InputHash; 2] = [InputHash::new([1; 32]), InputHash::new([2; 32])];
const SERVER_SEED: ServerSeed = ServerSeed::new([5; 32]);
const CONTRIBUTIONS: [[u8; 32]; 2] = [[6; 32], [7; 32]];

fn header() -> SessionHeader {
    SessionHeader {
        max_input_delay: MAX_DELAY,
        max_input_lead: MAX_LEAD,
        seed_commitment: SERVER_SEED.commitment(),
        players: ROOTS
            .iter()
            .zip(CONTRIBUTIONS)
            .map(|(&chain_root, seed_contribution)| SessionPlayer {
                chain_root,
                seed_contribution,
            })
            .collect(),
    }
}

/// What the players send before each tick, 0 to 4, as `(slot, stamp, payload)`. Player 0 sends
/// b, c, e, h, g; player 1 sends a, d, f, i, j.
const SCRIPT: [&[(u32, u64, &[u8])]; 5] = [
    &[(1, 0, b"a"), (0, 1, b"b")],
    &[(0, 1, b"c"), (1, 3, b"d")],
    &[],
    &[(1, 1, b"f"), (0, 0, b"e"), (0, 3, b"h"), (1, 6, b"i")],
    &[(0, 4, b"g"), (1, 4, b"j")],
];

/// The chained inputs the script's players send, grouped by the tick they arrive before.
fn chained() -> Vec<Vec<PlayerInput<'static>>> {
    let mut heads = ROOTS;
    let mut seqs = [0; 2];
    SCRIPT
        .iter()
        .map(|sent| {
            sent.iter()
                .map(|&(slot, stamp, payload)| {
                    let player = slot as usize;
                    let input = PlayerInput {
                        slot: PlayerSlot::new(slot),
                        seq: seqs[player],
                        stamp,
                        previous: heads[player],
                        payload,
                    };
                    heads[player] = input.hash();
                    seqs[player] += 1;
                    input
                })
                .collect()
        })
        .collect()
}

/// Each tick's applied inputs as `(slot, payload)`.
type PerTick = Vec<Vec<(u32, Vec<u8>)>>;

fn per_tick(ticks: &[&[(u32, &[u8])]]) -> PerTick {
    ticks
        .iter()
        .map(|inputs| {
            inputs
                .iter()
                .map(|&(slot, payload)| (slot, payload.to_vec()))
                .collect()
        })
        .collect()
}

#[derive(Debug)]
struct Recorded {
    log: SessionLog,
    applied: PerTick,
}

/// The first input a log refused, and the tick it arrived before.
#[derive(Debug, PartialEq, Eq)]
struct Refused {
    tick: u64,
    error: InputError,
}

/// Records `ticks` into a new log with `header`, sealing after each tick's inputs, as the server
/// does live and a verifier does from a published log.
fn record(header: SessionHeader, ticks: &[Vec<PlayerInput<'_>>]) -> Result<Recorded, Refused> {
    let mut log = SessionLog::new(header);
    let mut applied = Vec::new();
    for (tick, inputs) in (0..).zip(ticks) {
        for &input in inputs {
            log.record(input).map_err(|error| Refused { tick, error })?;
        }
        let names = log
            .seal_tick()
            .map(|input| (input.slot.get(), input.payload.to_vec()))
            .collect();
        applied.push(names);
    }
    Ok(Recorded { log, applied })
}

#[test]
fn inputs_apply_by_the_delay_rule_in_slot_order() {
    let mut log = SessionLog::new(header());
    let mut outcomes = Vec::new();
    for inputs in chained() {
        for input in inputs {
            outcomes.push((input.payload, log.record(input).unwrap()));
        }
        drop(log.seal_tick());
    }
    // Applied at max(stamp, next tick); late when that is more than 2 ticks after the stamp, early
    // when the stamp is more than 2 ticks ahead.
    assert_eq!(
        outcomes,
        [
            (&b"a"[..], Applied::At(0)), // stamp 0, before tick 0
            (b"b", Applied::At(1)),      // stamp 1 is ahead of tick 0
            (b"c", Applied::At(1)),      // stamp 1, before tick 1
            (b"d", Applied::At(3)),      // stamp 3, before tick 1: 2 ticks ahead, the most allowed
            (b"f", Applied::At(3)),      // stamp 1, before tick 3: 2 ticks late, the most allowed
            (b"e", Applied::Late),       // stamp 0, before tick 3: 3 ticks late
            (b"h", Applied::At(3)),      // stamp 3, before tick 3
            (b"i", Applied::Early),      // stamp 6, before tick 3: 3 ticks ahead
            (b"g", Applied::At(4)),      // stamp 4, before tick 4
            (b"j", Applied::At(4)),      // the chain goes on after the early i
        ]
    );
    assert_eq!(log.next_tick(), 5);

    let applied = record(header(), &chained()).unwrap().applied;
    // Tick 3 takes player 0's h before player 1's d and f, though h arrived last.
    let expected = per_tick(&[
        &[(1, b"a")],
        &[(0, b"b"), (0, b"c")],
        &[],
        &[(0, b"h"), (1, b"d"), (1, b"f")],
        &[(0, b"g"), (1, b"j")],
    ]);
    assert_eq!(applied, expected);
}

#[test]
fn a_replay_reads_the_inputs_back_in_order() {
    let sent = chained();
    let Recorded { log, applied: live } = record(header(), &sent).unwrap();
    let logged: Vec<Vec<_>> = (0..log.next_tick())
        .map(|tick| log.logged_before(tick).collect())
        .collect();
    assert_eq!(logged, sent);

    let replayed = record(log.header().clone(), &logged).unwrap();
    assert_eq!(replayed.applied, live);
}

/// A change to the sent inputs, and the first input it makes the log refuse.
#[derive(Debug)]
struct Tamper {
    name: &'static str,
    change: fn(&mut Vec<Vec<PlayerInput<'static>>>),
    refused: Refused,
}

#[test]
fn a_dropped_input_breaks_the_chain() {
    let cases = [
        Tamper {
            name: "player 0's c dropped: e links to c",
            change: |ticks| {
                ticks[1].remove(0);
            },
            refused: Refused {
                tick: 3,
                error: InputError::BrokenLink,
            },
        },
        Tamper {
            name: "player 0's e and h swapped",
            change: |ticks| ticks[3].swap(1, 2),
            refused: Refused {
                tick: 3,
                error: InputError::BrokenLink,
            },
        },
        Tamper {
            name: "player 1's a altered: d links to the real a",
            change: |ticks| ticks[0][0].payload = b"z",
            refused: Refused {
                tick: 1,
                error: InputError::BrokenLink,
            },
        },
        Tamper {
            name: "player 0's b sent twice",
            change: |ticks| {
                let b = ticks[0][1];
                ticks[0].push(b);
            },
            refused: Refused {
                tick: 0,
                error: InputError::BrokenLink,
            },
        },
        Tamper {
            name: "player 1's first seq changed",
            change: |ticks| ticks[0][0].seq = 1,
            refused: Refused {
                tick: 0,
                error: InputError::WrongSeq { expected: 0 },
            },
        },
        Tamper {
            name: "an input from a slot not in the header",
            change: |ticks| {
                ticks[2].push(PlayerInput {
                    slot: PlayerSlot::new(2),
                    seq: 0,
                    stamp: 2,
                    previous: ROOTS[0],
                    payload: b"x",
                });
            },
            refused: Refused {
                tick: 2,
                error: InputError::UnknownPlayer,
            },
        },
    ];
    for case in cases {
        let mut ticks = chained();
        (case.change)(&mut ticks);
        assert_eq!(
            record(header(), &ticks).err(),
            Some(case.refused),
            "{}",
            case.name
        );
    }

    // A refused input leaves the log unchanged: the real input still links afterwards.
    let sent = chained();
    let mut log = SessionLog::new(header());
    let c = sent[1][0];
    let b = sent[0][1];
    assert_eq!(log.record(c), Err(InputError::BrokenLink));
    assert_eq!(log.record(b), Ok(Applied::At(1)));
    assert_eq!(log.record(c), Ok(Applied::At(1)));
}

#[test]
fn the_segment_seed_comes_from_the_revealed_seed() {
    let digest = |parts: &[&[u8]]| {
        let mut hasher = Hasher::new();
        for part in parts {
            hasher.update(part);
        }
        *hasher.finalize().as_bytes()
    };
    assert_eq!(
        SERVER_SEED.commitment().as_bytes(),
        &digest(&[b"campfire/seed-commitment/v1", &[5; 32]])
    );
    let expected = SegmentSeed::new(digest(&[
        b"campfire/segment-seed/v1",
        &[5; 32],
        &[6; 32],
        &[7; 32],
    ]));
    assert_eq!(header().segment_seed(&SERVER_SEED), Ok(expected));
    assert_eq!(
        header().segment_seed(&ServerSeed::new([4; 32])),
        Err(SeedError::WrongSeed)
    );

    // Each contribution counts, and so does their slot order.
    let mut changed = header();
    changed.players[1].seed_contribution[31] ^= 1;
    let mut swapped = header();
    swapped.players.swap(0, 1);
    for other in [changed, swapped] {
        let seed = other.segment_seed(&SERVER_SEED).unwrap();
        assert_ne!(seed, expected, "{other:?}");
    }

    let mut log = SessionLog::new(header());
    assert_eq!(log.revealed_seed(), None);
    log.reveal_seed(SERVER_SEED);
    assert_eq!(log.revealed_seed(), Some(SERVER_SEED));
}

#[test]
#[should_panic(expected = "the server reveals the seed it committed to")]
fn revealing_another_seed_is_a_bug() {
    SessionLog::new(header()).reveal_seed(ServerSeed::new([4; 32]));
}

#[test]
fn the_hash_covers_every_field_in_its_layout() {
    let input = chained()[0][0];
    let mut hasher = Hasher::new();
    hasher
        .update(b"campfire/input-hash/v1")
        .update(&[2; 32])
        .update(&1_u32.to_le_bytes())
        .update(&0_u64.to_le_bytes())
        .update(&0_u64.to_le_bytes())
        .update(b"a");
    assert_eq!(input.hash().as_bytes(), hasher.finalize().as_bytes());

    let changed = [
        PlayerInput {
            slot: PlayerSlot::new(0),
            ..input
        },
        PlayerInput { seq: 1, ..input },
        PlayerInput { stamp: 1, ..input },
        PlayerInput {
            previous: ROOTS[0],
            ..input
        },
        PlayerInput {
            payload: b"b",
            ..input
        },
        PlayerInput {
            payload: b"",
            ..input
        },
    ];
    for other in changed {
        assert_ne!(other.hash(), input.hash(), "{other:?}");
    }
}
