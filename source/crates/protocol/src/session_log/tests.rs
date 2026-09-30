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

/// Inputs as `(slot, stamp, payload)`.
type Sends = &'static [(u32, u64, &'static [u8])];

/// What the players send before each tick, 0 to 4. Player 0 sends b, c, e, h, g; player 1 sends
/// a, d, f, i, j.
const SCRIPT: [Sends; 5] = [
    &[(1, 0, b"a"), (0, 1, b"b")],
    &[(0, 1, b"c"), (1, 3, b"d")],
    &[],
    &[(1, 1, b"f"), (0, 0, b"e"), (0, 3, b"h"), (1, 6, b"i")],
    &[(0, 4, b"g"), (1, 4, b"j")],
];

/// What the players send after tick 4, which stays unsealed: k applies at 6, l at 5.
const TAIL: Sends = &[(0, 6, b"k"), (1, 5, b"l")];

/// The chained inputs the script's players send, grouped by the tick they arrive before.
fn chained() -> Vec<Vec<PlayerInput<'static>>> {
    chain(&SCRIPT)
}

/// The chained inputs of `script` then `TAIL`, with the tail as the last group.
fn chained_with_tail() -> Vec<Vec<PlayerInput<'static>>> {
    chain(&[&SCRIPT[..], &[TAIL]].concat())
}

fn chain(script: &[Sends]) -> Vec<Vec<PlayerInput<'static>>> {
    let mut chains =
        [0, 1].map(|slot| InputChain::new(PlayerSlot::new(slot), ROOTS[slot as usize]));
    script
        .iter()
        .map(|sent| {
            sent.iter()
                .map(|&(slot, stamp, payload)| chains[slot as usize].extend(stamp, payload))
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

/// Records `ticks` into a new log with `header`, sealing after each tick's inputs, as the server
/// does live and a verifier does from a published log. The error is the first input refused, as
/// a log file gives it.
fn record(header: SessionHeader, ticks: &[Vec<PlayerInput<'_>>]) -> Result<Recorded, LogError> {
    let mut log = SessionLog::new(header);
    let mut applied = Vec::new();
    for (tick, inputs) in (0..).zip(ticks) {
        for &input in inputs {
            log.record(input)
                .map_err(|error| LogError::Input { tick, error })?;
        }
        applied.push(seal(&mut log));
    }
    Ok(Recorded { log, applied })
}

/// The applied inputs of the next tick, sealed now, as `(slot, payload)`.
fn seal(log: &mut SessionLog) -> Vec<(u32, Vec<u8>)> {
    log.seal_tick()
        .map(|input| (input.slot.get(), input.payload.to_vec()))
        .collect()
}

/// Each sealed tick's inputs as they were logged.
fn logged(log: &SessionLog) -> Vec<Vec<PlayerInput<'_>>> {
    (0..log.next_tick())
        .map(|tick| log.logged_before(tick).collect())
        .collect()
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
    let logged = logged(&log);
    assert_eq!(logged, sent);

    let replayed = record(log.header().clone(), &logged).unwrap();
    assert_eq!(replayed.applied, live);
}

/// A change to the sent inputs, and the first input it makes the log refuse.
#[derive(Debug)]
struct Tamper {
    name: &'static str,
    change: fn(&mut Vec<Vec<PlayerInput<'static>>>),
    refused: LogError,
}

#[test]
fn a_dropped_input_breaks_the_chain() {
    let cases = [
        Tamper {
            name: "player 0's c dropped: e links to c",
            change: |ticks| {
                ticks[1].remove(0);
            },
            refused: LogError::Input {
                tick: 3,
                error: InputError::BrokenLink,
            },
        },
        Tamper {
            name: "player 0's e and h swapped",
            change: |ticks| ticks[3].swap(1, 2),
            refused: LogError::Input {
                tick: 3,
                error: InputError::BrokenLink,
            },
        },
        Tamper {
            name: "player 1's a altered: d links to the real a",
            change: |ticks| ticks[0][0].payload = b"z",
            refused: LogError::Input {
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
            refused: LogError::Input {
                tick: 0,
                error: InputError::BrokenLink,
            },
        },
        Tamper {
            name: "player 1's first seq changed",
            change: |ticks| ticks[0][0].seq = 1,
            refused: LogError::Input {
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
            refused: LogError::Input {
                tick: 2,
                error: InputError::UnknownPlayer,
            },
        },
    ];
    for case in cases {
        let mut ticks = chained();
        (case.change)(&mut ticks);
        // A log file with the same inputs is refused for the same input.
        let file = frame(&header(), &ticks, &[], Some(SERVER_SEED));
        assert_eq!(
            SessionLog::decode(&file).err(),
            Some(case.refused.clone()),
            "{}",
            case.name
        );
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

/// A log file written piece by piece, apart from `SessionLog::encode`: `ticks` sealed, then `tail`.
fn frame(
    header: &SessionHeader,
    ticks: &[Vec<PlayerInput<'_>>],
    tail: &[PlayerInput<'_>],
    revealed: Option<ServerSeed>,
) -> Vec<u8> {
    let mut bytes = b"campfire/session-log/v1".to_vec();
    put(&mut bytes, header);
    put(&mut bytes, &u64::try_from(ticks.len()).unwrap());
    for group in ticks.iter().map(Vec::as_slice).chain([tail]) {
        put(&mut bytes, &u32::try_from(group.len()).unwrap());
        for input in group {
            put(&mut bytes, input);
        }
    }
    put(&mut bytes, &revealed);
    bytes
}

/// The script recorded and sealed, the tail recorded after the last tick, and the seed revealed.
fn published() -> SessionLog {
    let mut sent = chained_with_tail();
    let tail = sent.pop().unwrap();
    let mut log = record(header(), &sent).unwrap().log;
    for input in tail {
        log.record(input).unwrap();
    }
    log.reveal_seed(SERVER_SEED);
    log
}

fn encoded(log: &SessionLog) -> Vec<u8> {
    let mut bytes = vec![0xAA; 3];
    log.encode(&mut bytes);
    bytes
}

#[test]
fn a_log_file_decodes_to_the_same_log() {
    let mut log = published();
    let bytes = encoded(&log);
    let mut sent = chained_with_tail();
    let tail = sent.pop().unwrap();
    assert_eq!(bytes, frame(&header(), &sent, &tail, Some(SERVER_SEED)));

    let mut decoded = SessionLog::decode(&bytes).unwrap();
    assert_eq!(decoded.header(), log.header());
    assert_eq!(decoded.revealed_seed(), Some(SERVER_SEED));
    assert_eq!(decoded.next_tick(), 5);
    assert_eq!(logged(&decoded), sent);
    assert_eq!(encoded(&decoded), bytes);

    // The tail waits in both logs: l applies at tick 5, k at 6.
    let expected = per_tick(&[&[(1, b"l")], &[(0, b"k")]]);
    assert_eq!([seal(&mut log), seal(&mut log)].to_vec(), expected);
    assert_eq!([seal(&mut decoded), seal(&mut decoded)].to_vec(), expected);

    // An unpublished log, and one with no ticks, decode too.
    for log in [
        record(header(), &sent).unwrap().log,
        SessionLog::new(header()),
    ] {
        let decoded = SessionLog::decode(&encoded(&log)).unwrap();
        assert_eq!(decoded.revealed_seed(), None);
        assert_eq!(decoded.next_tick(), log.next_tick());
        assert_eq!(logged(&decoded), logged(&log));
    }
}

#[test]
fn a_log_file_has_its_layout() {
    let mut header = header();
    header.max_input_delay = 300;
    let mut log = SessionLog::new(header);
    let a = chained()[0][0];
    log.record(a).unwrap();
    drop(log.seal_tick());
    log.reveal_seed(SERVER_SEED);

    let expected = [
        &b"campfire/session-log/v1"[..],
        // Max input delay 300 = 0b10_0101100: varint 0xAC 0x02. Max input lead 2.
        &[0xAC, 0x02, 2],
        SERVER_SEED.commitment().as_bytes(),
        // 2 players: root and contribution each.
        &[2],
        &[1; 32],
        &[6; 32],
        &[2; 32],
        &[7; 32],
        // 1 tick with 1 input: slot 1, seq 0, stamp 0, previous, 1 payload byte.
        &[1, 1, 1, 0, 0],
        &[2; 32],
        &[1],
        b"a",
        // No input since the tick, then the revealed seed.
        &[0, 1],
        &[5; 32],
    ]
    .concat();
    assert_eq!(encoded(&log), expected);
}

#[test]
fn every_truncation_of_a_log_file_is_refused() {
    let bytes = encoded(&published());
    let tag = b"campfire/session-log/v1".len();
    for len in 0..bytes.len() {
        let expected = if len < tag {
            LogError::NotLog
        } else {
            LogError::Truncated
        };
        assert_eq!(
            SessionLog::decode(&bytes[..len]).err(),
            Some(expected),
            "truncated to {len} bytes"
        );
    }
}

#[test]
fn a_corrupt_log_file_is_refused_or_replays() {
    const FLIPS: [u8; 3] = [0x01, 0x80, 0xFF];
    let bytes = encoded(&published());
    let mut decoded = Vec::new();
    for at in 0..bytes.len() {
        for flip in FLIPS {
            let mut corrupt = bytes.clone();
            corrupt[at] ^= flip;
            let Ok(log) = SessionLog::decode(&corrupt) else {
                continue;
            };
            assert_eq!(encoded(&log), corrupt, "byte {at} ^ {flip:#x}");
            record(log.header().clone(), &logged(&log)).unwrap();
            decoded.push((at, flip));
        }
    }

    // Only what no chain link or commitment covers decodes, and only where a one-byte varint
    // stays one byte: the max delay and lead (2 ^ 1 = 3), every contribution byte, and the stamp
    // (6 ^ 1, 5 ^ 1) and the payload byte of k and l, each player's last input.
    let tag = b"campfire/session-log/v1".len();
    // After the delay, the lead, the commitment and the player count.
    let players = tag + 2 + 32 + 1;
    // Slot, seq, stamp, previous, payload length, payload.
    let input_len = 3 + 32 + 1 + 1;
    // Before the tail's two inputs: the reveal.
    let tail = bytes.len() - (1 + 32) - 2 * input_len;
    let mut expected = vec![(tag, 0x01), (tag + 1, 0x01)];
    for player in 0..2 {
        let contribution = players + 64 * player + 32;
        for at in contribution..contribution + 32 {
            expected.extend(FLIPS.map(|flip| (at, flip)));
        }
    }
    for input in [tail, tail + input_len] {
        expected.push((input + 2, 0x01));
        expected.extend(FLIPS.map(|flip| (input + input_len - 1, flip)));
    }
    expected.sort_unstable();
    assert_eq!(decoded, expected);
}

#[test]
fn flawed_log_files_are_refused() {
    let sent = chained();
    let valid = frame(&header(), &sent, &[], None);
    assert!(SessionLog::decode(&valid).is_ok());

    // The ticks count 5, after the tag and the header, as the overlong varint 0x85 0x00.
    let count_at =
        b"campfire/session-log/v1".len() + postcard::to_allocvec(&header()).unwrap().len();
    assert_eq!(valid[count_at], 5);
    let overlong = [&valid[..count_at], &[0x85, 0x00], &valid[count_at + 1..]].concat();

    let cases = [
        (b"not a log".to_vec(), LogError::NotLog),
        (valid[..valid.len() - 1].to_vec(), LogError::Truncated),
        ([valid.as_slice(), &[0]].concat(), LogError::Trailing),
        (
            [&valid[..valid.len() - 1], &[2]].concat(),
            LogError::Malformed(postcard::Error::DeserializeBadOption),
        ),
        (
            frame(&header(), &sent, &[], Some(ServerSeed::new([4; 32]))),
            LogError::WrongSeed,
        ),
        (overlong, LogError::NotCanonical),
    ];
    for (bytes, error) in cases {
        assert_eq!(SessionLog::decode(&bytes).err(), Some(error));
    }
}
