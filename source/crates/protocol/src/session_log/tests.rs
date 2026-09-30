use std::num::NonZeroU32;
use std::time::Duration;

use secp256k1::{Keypair, schnorr};

use super::*;
use crate::delegation::DelegationTerms;
use crate::input_hash::InputHash;
use crate::seed_chain::SeedChain;

const MAX_DELAY: u64 = 2;
const MAX_LEAD: u64 = 2;
const MAX_PAYLOAD_LEN: u32 = 4;
const MAX_INPUTS_PER_TICK: u32 = 2;
const SERVER_KEY: [u8; 32] = [41; 32];
/// Above 127, so its varint takes two bytes.
const TICK_HZ: NonZeroU32 = NonZeroU32::new(300).unwrap();
/// Two segments: the root `[5; 32]` is segment 1's seed, and its hash segment 0's.
const SEED_CHAIN: SeedChain = SeedChain::new([5; 32], NonZeroU32::new(2).unwrap());
const CONTRIBUTIONS: [[u8; 32]; 2] = [[6; 32], [7; 32]];
const RELEASE: &str = "0.1.0";
const MODE: [u8; 32] = [51; 32];
const DEPENDENCIES: [[u8; 32]; 2] = [[52; 32], [53; 32]];
/// BIP-340 signing without auxiliary randomness is deterministic, so every run signs alike.
const AUX: [u8; 32] = [0; 32];

fn secret(byte: u32) -> secp256k1::SecretKey {
    secp256k1::SecretKey::from_byte_array(&[u8::try_from(byte).unwrap(); 32]).unwrap()
}

/// Player `slot`'s session key.
fn session_key(slot: u32) -> Keypair {
    Keypair::from_secret_key(&Secp256k1::new(), &secret(21 + slot))
}

/// Player `slot`'s delegation of their session key in this session, with `change` applied to its
/// terms.
fn delegation_with(slot: u32, change: impl FnOnce(&mut DelegationTerms)) -> Delegation {
    let mut terms = DelegationTerms {
        session_key: session_key(slot).x_only_public_key().0,
        server_key: SERVER_KEY,
        session_id: session_id(),
        seed_contribution: CONTRIBUTIONS[slot as usize],
        expiration: 1_700_086_400,
    };
    change(&mut terms);
    let main_key = Keypair::from_secret_key(&Secp256k1::new(), &secret(11 + slot));
    Delegation::sign(&Secp256k1::new(), &main_key, &terms, 1_700_000_000, &AUX)
}

fn terms() -> SessionTerms {
    SessionTerms {
        server_key: SERVER_KEY,
        tick_hz: TICK_HZ,
        max_input_delay: MAX_DELAY,
        max_input_lead: MAX_LEAD,
        max_payload_len: MAX_PAYLOAD_LEN,
        max_inputs_per_tick: MAX_INPUTS_PER_TICK,
        seed_commitment: SEED_CHAIN.commitment(),
        release: RELEASE.to_owned(),
        mode: MODE,
        dependencies: DEPENDENCIES.to_vec(),
    }
}

fn session_id() -> SessionId {
    terms().session_id()
}

fn delegation(slot: u32) -> Delegation {
    delegation_with(slot, |_| ())
}

/// The first segment's server seed, which the log reveals.
fn server_seed() -> ServerSeed {
    SEED_CHAIN.seed(0)
}

fn root(slot: u32) -> InputHash {
    delegation(slot).chain_root()
}

fn header() -> SessionHeader {
    SessionHeader {
        terms: terms(),
        players: (0..2).map(delegation).collect(),
    }
}

/// A packet as a player sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sent<'a> {
    inputs: Vec<PlayerInput<'a>>,
    signature: ChainSignature,
}

/// Inputs as `(slot, stamp, payload)`; a run of one slot's inputs within a tick is one packet.
type Sends = &'static [(u32, u64, &'static [u8])];

/// What the players send before each tick, 0 to 4. Player 0 sends b, c, e, h, g; player 1 sends
/// a, d, f, i, j. Before tick 3, e and h are one packet.
const SCRIPT: [Sends; 5] = [
    &[(1, 0, b"a"), (0, 1, b"b")],
    &[(0, 1, b"c"), (1, 3, b"d")],
    &[],
    &[(1, 1, b"f"), (0, 0, b"e"), (0, 3, b"h"), (1, 6, b"i")],
    &[(0, 4, b"g"), (1, 4, b"j")],
];

/// What the players send after tick 4, which stays unsealed: k applies at 6, l at 5.
const TAIL: Sends = &[(0, 6, b"k"), (1, 5, b"l")];

/// The signed packets the script's players send, grouped by the tick they arrive before.
fn chained() -> Vec<Vec<Sent<'static>>> {
    chain(&SCRIPT)
}

/// The signed packets of `script` then `TAIL`, with the tail as the last group.
fn chained_with_tail() -> Vec<Vec<Sent<'static>>> {
    chain(&[&SCRIPT[..], &[TAIL]].concat())
}

fn chain(script: &[Sends]) -> Vec<Vec<Sent<'static>>> {
    let secp = Secp256k1::new();
    let keys = [0, 1].map(session_key);
    let mut chains = [0, 1].map(|slot| InputChain::new(PlayerSlot::new(slot), root(slot)));
    script
        .iter()
        .map(|sends| {
            let mut packets: Vec<Sent<'static>> = Vec::new();
            let mut last_slot = None;
            for &(slot, stamp, payload) in *sends {
                let chain = &mut chains[slot as usize];
                let input = chain.extend(stamp, payload);
                let signature = chain.sign(&secp, &keys[slot as usize], session_id(), &AUX);
                match packets.last_mut() {
                    Some(packet) if last_slot == Some(slot) => {
                        packet.inputs.push(input);
                        packet.signature = signature;
                    }
                    _ => packets.push(Sent {
                        inputs: vec![input],
                        signature,
                    }),
                }
                last_slot = Some(slot);
            }
            packets
        })
        .collect()
}

/// Player `slot`'s first packets, `(stamp, payload)` each, chained from the root and signed by
/// the session key of `signer`.
fn resent(slot: u32, packets: &[&[(u64, &'static [u8])]], signer: u32) -> Vec<Sent<'static>> {
    let secp = Secp256k1::new();
    let mut chain = InputChain::new(PlayerSlot::new(slot), root(slot));
    packets
        .iter()
        .map(|sends| {
            let inputs = sends
                .iter()
                .map(|&(stamp, payload)| chain.extend(stamp, payload))
                .collect();
            let signature = chain.sign(&secp, &session_key(signer), session_id(), &AUX);
            Sent { inputs, signature }
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

fn record(header: SessionHeader, ticks: &[Vec<Sent<'_>>]) -> Result<Recorded, LogError> {
    record_into(SessionLog::new(header).unwrap(), ticks)
}

/// Records `ticks` into `log`, sealing after each tick's packets, as the server does live and a
/// verifier does from a published log. The error is the first packet refused, as a log file gives
/// it.
fn record_into(mut log: SessionLog, ticks: &[Vec<Sent<'_>>]) -> Result<Recorded, LogError> {
    let mut outcomes = Vec::new();
    let mut applied = Vec::new();
    for (tick, packets) in (0..).zip(ticks) {
        for packet in packets {
            log.record(
                packet.inputs.iter().copied(),
                &packet.signature,
                &mut outcomes,
            )
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

/// `log` rewound and replayed to its last sealed tick: each tick's applied inputs, and the log.
fn replayed(log: SessionLog) -> Recorded {
    let ticks = log.next_tick();
    let mut log = log.rewound();
    let applied = (0..ticks).map(|_| seal(&mut log)).collect();
    Recorded { log, applied }
}

#[test]
fn inputs_apply_by_the_delay_rule_in_slot_order() {
    let mut log = SessionLog::new(header()).unwrap();
    let mut outcomes = Vec::new();
    let mut applied = Vec::new();
    for packets in chained() {
        for packet in packets {
            log.record(
                packet.inputs.iter().copied(),
                &packet.signature,
                &mut applied,
            )
            .unwrap();
            let payloads = packet.inputs.iter().map(|input| input.payload);
            outcomes.extend(payloads.zip(applied.iter().copied()));
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

    // Player 0 sends 1, 1, 0, 2 and 1 inputs before the ticks: the count per tick starts again
    // at each seal, or the 2 before tick 3 would pass the max of 2.
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
fn a_rewound_log_replays_the_same_ticks_and_ends_as_it_was() {
    let log = published();
    let bytes = encoded(&log);
    let Recorded { mut log, applied } = replayed(log);
    let expected = per_tick(&[
        &[(1, b"a")],
        &[(0, b"b"), (0, b"c")],
        &[],
        &[(0, b"h"), (1, b"d"), (1, b"f")],
        &[(0, b"g"), (1, b"j")],
    ]);
    assert_eq!(applied, expected);
    // Caught up, the log is as it was: the same file, and the tail waits to apply.
    assert_eq!(encoded(&log), bytes);
    assert_eq!(
        [seal(&mut log), seal(&mut log)].to_vec(),
        per_tick(&[&[(1, b"l")], &[(0, b"k")]])
    );

    // A log with no sealed tick rewinds to itself.
    let empty = SessionLog::new(header()).unwrap().rewound();
    assert_eq!(empty.next_tick(), 0);
}

/// A change to the sent packets, and the first packet it makes the log refuse.
#[derive(Debug)]
struct Tamper {
    name: &'static str,
    change: fn(&mut Vec<Vec<Sent<'static>>>),
    refused: LogError,
}

/// The tampered sends, each with the packet it makes the log refuse.
fn tampers() -> [Tamper; 11] {
    let refused = |tick, error| LogError::Input { tick, error };
    [
        Tamper {
            name: "player 0's c dropped: the head after e and h is not the one signed",
            change: |ticks| drop(ticks[1].remove(0)),
            refused: refused(3, InputError::BadSignature),
        },
        Tamper {
            name: "player 0's e and h swapped in their packet",
            change: |ticks| ticks[3][1].inputs.swap(0, 1),
            refused: refused(3, InputError::BadSignature),
        },
        Tamper {
            name: "player 1's a altered: its signature no longer holds",
            change: |ticks| ticks[0][0].inputs[0].payload = b"z",
            refused: refused(0, InputError::BadSignature),
        },
        Tamper {
            name: "player 0's b signed over the chain head after c",
            change: |ticks| ticks[0][1].signature = ticks[1][0].signature,
            refused: refused(0, InputError::BadSignature),
        },
        Tamper {
            name: "player 1's a signed by player 0's session key",
            change: |ticks| ticks[0][0] = resent(1, &[&[(0, b"a")]], 0).remove(0),
            refused: refused(0, InputError::BadSignature),
        },
        Tamper {
            name: "player 0's b sent twice",
            change: |ticks| {
                let b = ticks[0][1].clone();
                ticks[0].push(b);
            },
            refused: refused(0, InputError::BadSignature),
        },
        Tamper {
            name: "an input from a slot not in the header",
            change: |ticks| {
                let signature = ticks[0][0].signature;
                ticks[2].push(Sent {
                    inputs: vec![PlayerInput {
                        slot: PlayerSlot::new(2),
                        stamp: 2,
                        payload: b"x",
                    }],
                    signature,
                });
            },
            refused: refused(2, InputError::UnknownPlayer),
        },
        Tamper {
            name: "a packet with no input",
            change: |ticks| {
                let signature = ticks[0][0].signature;
                ticks[2].push(Sent {
                    inputs: Vec::new(),
                    signature,
                });
            },
            refused: refused(2, InputError::EmptyPacket),
        },
        Tamper {
            name: "player 1's a with 5 payload bytes, one over the max",
            change: |ticks| ticks[0][0] = resent(1, &[&[(0, b"abcde")]], 1).remove(0),
            refused: refused(0, InputError::PayloadTooLarge),
        },
        Tamper {
            name: "player 1 sends 3 inputs in one packet before tick 0, one over the max",
            change: |ticks| {
                ticks[0][0] = resent(1, &[&[(0, b"a"), (0, b"x"), (0, b"y")]], 1).remove(0);
            },
            refused: refused(0, InputError::TooManyInputs),
        },
        Tamper {
            name: "player 1 sends 2 inputs, then 1 more, before tick 0",
            change: |ticks| {
                let packets = resent(1, &[&[(0, b"a"), (0, b"x")], &[(0, b"y")]], 1);
                drop(ticks[0].splice(0..1, packets));
            },
            refused: refused(0, InputError::TooManyInputs),
        },
    ]
}

#[test]
fn a_tampered_packet_is_refused() {
    for case in tampers() {
        let mut ticks = chained();
        (case.change)(&mut ticks);
        // A log file with the same packets is refused for the same packet.
        let file = frame(&header(), &ticks, &[], Some(server_seed()));
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
}

#[test]
fn a_packet_past_the_position_bound_is_refused() {
    // The log's position bound, lowered from 2³² − 1 here: the 10th input, j, needs 10
    // inputs and 10 payload bytes.
    let bounded = |bound| {
        let mut log = SessionLog::new(header()).unwrap();
        log.position_bound = bound;
        record_into(log, &chained()).err()
    };
    let full = LogError::Input {
        tick: 4,
        error: InputError::LogFull,
    };
    assert_eq!(bounded(9), Some(full.clone()));
    assert_eq!(bounded(10), None);
    let file = encoded(&record(header(), &chained()).unwrap().log);
    assert_eq!(SessionLog::decode_within(&file, 9).err(), Some(full));
    assert!(SessionLog::decode_within(&file, 10).is_ok());
    // The payload bytes count on their own: 1 input of 4 bytes passes a bound of 3.
    let mut log = SessionLog::new(header()).unwrap();
    log.position_bound = 3;
    let oversized = resent(1, &[&[(0, b"abcd")]], 1).remove(0);
    let mut applied = Vec::new();
    assert_eq!(
        log.record(
            oversized.inputs.iter().copied(),
            &oversized.signature,
            &mut applied
        ),
        Err(InputError::LogFull)
    );
}

#[test]
fn a_refused_packet_leaves_the_log_unchanged() {
    // The real packets still link after the refused ones, and a refused packet does not count
    // towards the max per tick.
    let sent = chained();
    let mut applied = Vec::new();
    let mut log = SessionLog::new(header()).unwrap();
    let submit = |log: &mut SessionLog, packet: &Sent<'_>, applied: &mut Vec<Applied>| {
        log.record(packet.inputs.iter().copied(), &packet.signature, applied)
    };
    let (b, c) = (&sent[0][1], &sent[1][0]);
    let b_over_c = Sent {
        signature: c.signature,
        ..b.clone()
    };
    let three = resent(0, &[&[(1, b"b"), (1, b"x"), (1, b"y")]], 0).remove(0);
    assert_eq!(
        submit(&mut log, c, &mut applied),
        Err(InputError::BadSignature)
    );
    assert_eq!(
        submit(&mut log, &b_over_c, &mut applied),
        Err(InputError::BadSignature)
    );
    assert_eq!(
        submit(&mut log, &three, &mut applied),
        Err(InputError::TooManyInputs)
    );
    assert_eq!(submit(&mut log, b, &mut applied), Ok(()));
    assert_eq!(applied, [Applied::At(1)]);
    assert_eq!(submit(&mut log, c, &mut applied), Ok(()));
    assert_eq!(applied, [Applied::At(1)]);
}

#[test]
fn a_delegation_for_another_server_or_session_is_refused() {
    let cases = [
        (
            delegation_with(1, |terms| terms.server_key = [42; 32]),
            DelegationError::OtherServer,
        ),
        (
            delegation_with(1, |terms| terms.session_id = SessionId::new([32; 32])),
            DelegationError::OtherSession,
        ),
    ];
    for (delegation, error) in cases {
        let mut other = header();
        other.players[1] = delegation;
        refuses(&other, error);
    }

    // The session id hashes the terms, so a change to any of them leaves every delegation
    // naming another session.
    let changes: [fn(&mut SessionTerms); 11] = [
        |terms| terms.server_key[0] ^= 1,
        |terms| terms.tick_hz = NonZeroU32::new(301).unwrap(),
        |terms| terms.max_input_delay += 1,
        |terms| terms.max_input_lead += 1,
        |terms| terms.max_payload_len += 1,
        |terms| terms.max_inputs_per_tick += 1,
        |terms| terms.seed_commitment = SeedChain::new([6; 32], NonZeroU32::MIN).commitment(),
        |terms| terms.release.push('1'),
        |terms| terms.mode[31] ^= 1,
        |terms| terms.dependencies[1][0] ^= 1,
        |terms| terms.dependencies.swap(0, 1),
    ];
    for change in changes {
        let mut other = header();
        change(&mut other.terms);
        let error = if other.terms.server_key == SERVER_KEY {
            DelegationError::OtherSession
        } else {
            DelegationError::OtherServer
        };
        let refused = HeaderError::Delegation {
            slot: PlayerSlot::new(0),
            error,
        };
        assert_eq!(SessionLog::new(other).err(), Some(refused));
    }

    // 300 ticks a second last 3 333 333 ns each.
    assert_eq!(terms().tick_length(), Duration::from_nanos(3_333_333));

    // The id, as design 05 spells it.
    let mut spelled = Hasher::new();
    spelled
        .update(b"campfire/session-id/v1")
        .update(&SERVER_KEY)
        .update(&300_u32.to_le_bytes())
        .update(&2_u64.to_le_bytes())
        .update(&2_u64.to_le_bytes())
        .update(&4_u32.to_le_bytes())
        .update(&2_u32.to_le_bytes())
        .update(SEED_CHAIN.commitment().as_bytes())
        .update(&5_u64.to_le_bytes())
        .update(b"0.1.0")
        .update(&[51; 32])
        .update(&2_u64.to_le_bytes())
        .update(&[52; 32])
        .update(&[53; 32]);
    assert_eq!(session_id().as_bytes(), spelled.finalize().as_bytes());
}

/// `other` fails to start a log, and to decode, as player 1's delegation fails with `error`.
fn refuses(other: &SessionHeader, error: DelegationError) {
    let refused = HeaderError::Delegation {
        slot: PlayerSlot::new(1),
        error,
    };
    assert_eq!(SessionLog::new(other.clone()).err(), Some(refused));
    assert_eq!(
        SessionLog::decode(&frame(other, &[], &[], None)).err(),
        Some(LogError::Header(refused))
    );
}

#[test]
fn each_segment_seed_comes_from_its_chain_seed_and_the_signed_contributions() {
    let digest = |parts: &[&[u8]]| {
        let mut hasher = Hasher::new();
        for part in parts {
            hasher.update(part);
        }
        *hasher.finalize().as_bytes()
    };
    // s_1 is the root, s_0 its hash, and the commitment the hash of s_0.
    let (s0, s1) = (SEED_CHAIN.seed(0), SEED_CHAIN.seed(1));
    assert_eq!(s1.as_bytes(), &[5; 32]);
    assert_eq!(
        s0.as_bytes(),
        &digest(&[b"campfire/seed-chain/v1", &[5; 32]])
    );
    let commitment = SEED_CHAIN.commitment();
    assert_eq!(
        commitment.as_bytes(),
        &digest(&[b"campfire/seed-chain/v1", s0.as_bytes()])
    );
    // A seed checks for its own segment only: 1 hash leads from s_0 to the commitment, 2 from s_1.
    assert!(s0.check(0, &commitment));
    assert!(s1.check(1, &commitment));
    assert!(!s1.check(0, &commitment));
    assert!(!s0.check(1, &commitment));
    // A chain of one segment has its root as that segment's seed.
    let single = SeedChain::new([5; 32], NonZeroU32::MIN);
    assert_eq!(single.seed(0), s1);
    assert!(s1.check(0, &single.commitment()));

    let expected = [0, 1].map(|segment: u32| {
        SegmentSeed::new(digest(&[
            b"campfire/segment-seed/v1",
            &segment.to_le_bytes(),
            SEED_CHAIN.seed(segment).as_bytes(),
            &[6; 32],
            &[7; 32],
        ]))
    });
    assert_ne!(expected[0], expected[1]);
    for (segment, expected) in (0..).zip(expected) {
        let seed = header().segment_seed(segment, &SEED_CHAIN.seed(segment));
        assert_eq!(seed, Ok(expected));
    }
    for (segment, server_seed) in [(0, s1), (1, s0), (0, ServerSeed::new([4; 32]))] {
        assert_eq!(
            header().segment_seed(segment, &server_seed),
            Err(SeedError::WrongSeed),
            "{segment}: {server_seed:?}"
        );
    }

    // Each signed contribution counts in every segment, and so does their slot order.
    let mut changed = header();
    changed.players[1] = delegation_with(1, |terms| terms.seed_contribution[31] ^= 1);
    let mut swapped = header();
    swapped.players.swap(0, 1);
    for other in [changed, swapped] {
        for (segment, expected) in (0..).zip(expected) {
            let seed = other.segment_seed(segment, &SEED_CHAIN.seed(segment));
            assert_ne!(seed.unwrap(), expected, "{segment}: {other:?}");
        }
    }

    let mut log = SessionLog::new(header()).unwrap();
    assert_eq!(log.revealed_seed(), None);
    log.reveal_seed(s0);
    assert_eq!(log.revealed_seed(), Some(s0));
}

#[test]
#[should_panic(expected = "the server reveals the seed it committed to")]
fn revealing_another_seed_is_a_bug() {
    SessionLog::new(header())
        .unwrap()
        .reveal_seed(SEED_CHAIN.seed(1));
}

#[test]
#[should_panic(expected = "segment 2 is past the chain's 2 segments")]
fn a_seed_past_the_chain_is_a_bug() {
    SEED_CHAIN.seed(2);
}

#[test]
fn the_hash_and_the_signature_cover_every_field_in_their_layout() {
    let a = chained()[0][0].clone();
    let head = |slot: u32, root: InputHash, inputs: &[(u64, &[u8])]| {
        let mut chain = InputChain::new(PlayerSlot::new(slot), root);
        for &(stamp, payload) in inputs {
            chain.extend(stamp, payload);
        }
        chain.head()
    };
    // Player 1's a: `domain ‖ root ‖ u32 slot 1 ‖ u64 seq 0 ‖ u64 stamp 0 ‖ "a"`.
    let mut hasher = Hasher::new();
    hasher
        .update(b"campfire/input-hash/v1")
        .update(root(1).as_bytes())
        .update(&1_u32.to_le_bytes())
        .update(&0_u64.to_le_bytes())
        .update(&0_u64.to_le_bytes())
        .update(b"a");
    let after_a = head(1, root(1), &[(0, b"a")]);
    assert_eq!(after_a.as_bytes(), hasher.finalize().as_bytes());
    // Each field counts: another slot, root, stamp or payload, and a second input with seq 1
    // links to the first.
    let mut hasher = Hasher::new();
    hasher
        .update(b"campfire/input-hash/v1")
        .update(after_a.as_bytes())
        .update(&1_u32.to_le_bytes())
        .update(&1_u64.to_le_bytes())
        .update(&0_u64.to_le_bytes())
        .update(b"a");
    assert_eq!(
        head(1, root(1), &[(0, b"a"), (0, b"a")]).as_bytes(),
        hasher.finalize().as_bytes()
    );
    for other in [
        head(0, root(1), &[(0, b"a")]),
        head(1, root(0), &[(0, b"a")]),
        head(1, root(1), &[(1, b"a")]),
        head(1, root(1), &[(0, b"b")]),
        head(1, root(1), &[(0, b"")]),
    ] {
        assert_ne!(other, after_a);
    }

    // The session key signs `domain ‖ session id ‖ u32 slot ‖ u64 seq ‖ head`, the seq being
    // the last input's: the signature holds over that message as secp256k1 checks it.
    let secp = Secp256k1::new();
    let message = [
        &b"campfire/input/v1"[..],
        session_id().as_bytes(),
        &1_u32.to_le_bytes(),
        &0_u64.to_le_bytes(),
        after_a.as_bytes(),
    ]
    .concat();
    let key = session_key(1).x_only_public_key().0;
    let signature = schnorr::Signature::from_byte_array(a.signature.to_bytes());
    assert_eq!(secp.verify_schnorr(&signature, &message, &key), Ok(()));

    // Another session, key or head, and the signature does not hold.
    let mut chain = InputChain::new(PlayerSlot::new(1), root(1));
    chain.extend(0, b"a");
    assert!(chain.signed_by(&secp, &key, session_id(), &a.signature));
    let other_session = SessionId::new([32; 32]);
    assert!(!chain.signed_by(&secp, &key, other_session, &a.signature));
    let other_key = session_key(0).x_only_public_key().0;
    assert!(!chain.signed_by(&secp, &other_key, session_id(), &a.signature));
    chain.extend(0, b"b");
    assert!(!chain.signed_by(&secp, &key, session_id(), &a.signature));
}

/// A log file written piece by piece, apart from `SessionLog::encode`, from any header: `ticks`
/// sealed, then `tail`.
fn frame(
    header: &SessionHeader,
    ticks: &[Vec<Sent<'_>>],
    tail: &[Sent<'_>],
    revealed: Option<ServerSeed>,
) -> Vec<u8> {
    let mut bytes = b"campfire/session-log/v1".to_vec();
    let terms = &header.terms;
    put(&mut bytes, &terms.server_key);
    put(&mut bytes, &terms.tick_hz);
    put(&mut bytes, &terms.max_input_delay);
    put(&mut bytes, &terms.max_input_lead);
    put(&mut bytes, &terms.max_payload_len);
    put(&mut bytes, &terms.max_inputs_per_tick);
    put(&mut bytes, &terms.seed_commitment);
    put(&mut bytes, terms.release.as_str());
    put(&mut bytes, &terms.mode);
    put(&mut bytes, &terms.dependencies);
    put(&mut bytes, &u32::try_from(header.players.len()).unwrap());
    for delegation in &header.players {
        put(&mut bytes, delegation.json());
    }
    put(&mut bytes, &u64::try_from(ticks.len()).unwrap());
    for packets in ticks.iter().map(Vec::as_slice).chain([tail]) {
        put(&mut bytes, &u32::try_from(packets.len()).unwrap());
        for packet in packets {
            let slot = packet.inputs.first().map_or(0, |input| input.slot.get());
            put(&mut bytes, &slot);
            put(&mut bytes, &u32::try_from(packet.inputs.len()).unwrap());
            for input in &packet.inputs {
                put(&mut bytes, &input.stamp);
                put(&mut bytes, input.payload);
            }
            put(&mut bytes, &packet.signature);
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
    let mut applied = Vec::new();
    for packet in tail {
        log.record(
            packet.inputs.iter().copied(),
            &packet.signature,
            &mut applied,
        )
        .unwrap();
    }
    log.reveal_seed(server_seed());
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
    assert_eq!(bytes, frame(&header(), &sent, &tail, Some(server_seed())));

    let mut decoded = SessionLog::decode(&bytes).unwrap();
    assert_eq!(decoded.header(), log.header());
    assert_eq!(decoded.revealed_seed(), Some(server_seed()));
    assert_eq!(decoded.next_tick(), 5);
    assert_eq!(encoded(&decoded), bytes);
    let live = record(header(), &sent).unwrap().applied;
    let decoded_again = SessionLog::decode(&bytes).unwrap();
    assert_eq!(replayed(decoded_again).applied, live);

    // The tail waits in both logs: l applies at tick 5, k at 6.
    let expected = per_tick(&[&[(1, b"l")], &[(0, b"k")]]);
    assert_eq!([seal(&mut log), seal(&mut log)].to_vec(), expected);
    assert_eq!([seal(&mut decoded), seal(&mut decoded)].to_vec(), expected);

    // An unpublished log, and one with no ticks, decode too.
    let empty = SessionLog::new(header()).unwrap();
    for log in [record(header(), &sent).unwrap().log, empty] {
        let bytes = encoded(&log);
        let decoded = SessionLog::decode(&bytes).unwrap();
        assert_eq!(decoded.revealed_seed(), None);
        assert_eq!(decoded.next_tick(), log.next_tick());
        assert_eq!(encoded(&decoded), bytes);
    }
}

/// Postcard's varint of `len`, for a length below 2¹⁴: the low 7 bits with the high bit set,
/// then the rest.
fn varint(len: usize) -> Vec<u8> {
    assert!((128..1 << 14).contains(&len), "{len} takes 2 bytes");
    vec![
        u8::try_from(len & 0x7F).unwrap() | 0x80,
        u8::try_from(len >> 7).unwrap(),
    ]
}

#[test]
fn a_log_file_has_its_layout() {
    let mut log = SessionLog::new(header()).unwrap();
    let a = chained()[0][0].clone();
    let mut applied = Vec::new();
    log.record(a.inputs.iter().copied(), &a.signature, &mut applied)
        .unwrap();
    drop(log.seal_tick());
    log.reveal_seed(server_seed());

    let [first, second] = [0, 1].map(delegation);
    let expected = [
        &b"campfire/session-log/v1"[..],
        // The terms, and no session id: it is their hash.
        &[41; 32],
        // 300 ticks a second = 0b10_0101100: varint 0xAC 0x02. Max input delay 2, lead 2,
        // payload length 4, inputs per tick 2.
        &[0xAC, 0x02, 2, 2, 4, 2],
        SEED_CHAIN.commitment().as_bytes(),
        // The release as a length and UTF-8, the mode's fingerprint, and the count and
        // fingerprints of its 2 dependencies.
        &[5],
        b"0.1.0",
        &[51; 32],
        &[2],
        &[52; 32],
        &[53; 32],
        // 2 players: the delegation's JSON as a length and UTF-8.
        &[2],
        &varint(first.json().len()),
        first.json().as_bytes(),
        &varint(second.json().len()),
        second.json().as_bytes(),
        // 1 tick with 1 packet: slot 1, 1 input: stamp 0, 1 payload byte.
        &[1, 1, 1, 1, 0, 1],
        b"a",
        &a.signature.to_bytes(),
        // No packet since the tick, then the revealed seed.
        &[0, 1],
        server_seed().as_bytes(),
    ]
    .concat();
    assert_eq!(encoded(&log), expected);

    // A rate of 0 ticks a second does not decode.
    let tick_at = b"campfire/session-log/v1".len() + 32;
    let zero_rate = [&expected[..tick_at], &[0], &expected[tick_at + 2..]].concat();
    assert!(matches!(
        SessionLog::decode(&zero_rate),
        Err(LogError::Malformed(_))
    ));
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
fn every_flip_of_a_log_file_is_refused() {
    // A signature, a chain link or the commitment covers every byte: the session id hashes the
    // terms, each main key signs its delegation with the contribution, each session key its
    // inputs, and the reveal checks against the commitment.
    let bytes = encoded(&published());
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
fn flawed_log_files_are_refused() {
    let sent = chained();
    let valid = frame(&header(), &sent, &[], None);
    assert!(SessionLog::decode(&valid).is_ok());

    // The ticks count 5, after the tag and the header, as the overlong varint 0x85 0x00: the
    // file of a log with no tick ends with the count 0, no packet and no reveal.
    let count_at = encoded(&SessionLog::new(header()).unwrap()).len() - 3;
    assert_eq!(valid[count_at], 5);
    let overlong = [&valid[..count_at], &[0x85, 0x00], &valid[count_at + 1..]].concat();

    // Player 1's delegation signature, its first hex digit moved to its next digit.
    let json = delegation(1).json().to_owned();
    let json_at = valid
        .windows(json.len())
        .position(|window| window == json.as_bytes())
        .unwrap();
    let sig_at = json_at + json.find("\"sig\":\"").unwrap() + "\"sig\":\"".len();
    let mut forged = valid.clone();
    forged[sig_at] = if forged[sig_at] == b'0' { b'1' } else { b'0' };

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
        // Segment 1's seed is in the chain, but not the seed of the one segment the log holds.
        (
            frame(&header(), &sent, &[], Some(SEED_CHAIN.seed(1))),
            LogError::WrongSeed,
        ),
        (overlong, LogError::NotCanonical),
        (
            forged,
            LogError::Header(HeaderError::Delegation {
                slot: PlayerSlot::new(1),
                error: DelegationError::BadSignature,
            }),
        ),
    ];
    for (bytes, error) in cases {
        assert_eq!(SessionLog::decode(&bytes).err(), Some(error));
    }
}
