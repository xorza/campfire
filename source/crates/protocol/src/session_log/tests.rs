use std::num::NonZeroU32;

use secp256k1::{Keypair, XOnlyPublicKey};

use super::*;
use crate::delegation::DelegationTerms;
use crate::delegation::error::{DelegationError, ScopeError};
use crate::fingerprint::Fingerprint;
use crate::input_hash::InputHash;
use crate::seed_chain::SeedChain;

const MAX_DELAY: u64 = 2;
const MAX_LEAD: u64 = 2;
const MAX_PAYLOAD_LEN: u32 = 4;
const MAX_INPUTS_PER_TICK: u32 = 2;
/// Above 127, so its varint takes two bytes.
const TICK_HZ: NonZeroU32 = NonZeroU32::new(300).unwrap();
/// Two segments: the root `[5; 32]` is segment 1's seed, and its hash segment 0's.
const SEED_CHAIN: SeedChain = SeedChain::new([5; 32], NonZeroU32::new(2).unwrap());
const CONTRIBUTIONS: [[u8; 32]; 2] = [[6; 32], [7; 32]];
const RELEASE: &str = "0.1.0";
const MODE: Fingerprint = Fingerprint::new([51; 32]);
const DEPENDENCIES: [Fingerprint; 2] = [Fingerprint::new([52; 32]), Fingerprint::new([53; 32])];
/// BIP-340 signing without auxiliary randomness is deterministic, so every run signs alike.
const AUX: [u8; 32] = [0; 32];

/// An x-only key of the bytes `[byte; 32]`, which must be the x of a point on the curve.
fn x_only(byte: u8) -> XOnlyPublicKey {
    XOnlyPublicKey::from_byte_array(&[byte; 32]).unwrap()
}

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
        server_key: x_only(41),
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
        server_key: x_only(41),
        tick_hz: TICK_HZ,
        max_input_delay: Ticks::new(MAX_DELAY),
        max_input_lead: Ticks::new(MAX_LEAD),
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
    signature: Signature,
}

impl Sent<'_> {
    /// Records the packet into `log`, as the server does when it arrives.
    fn submit(&self, log: &mut SessionLog, applied: &mut Vec<Applied>) -> Result<(), InputError> {
        log.record(self.inputs.iter().copied(), &self.signature, applied)
    }
}

/// A new log of `header()`.
fn new_log() -> SessionLog {
    SessionLog::new(header()).unwrap()
}

/// What `chained()` applies in its five ticks: tick 3 takes player 0's g and h before player
/// 1's f, though f arrived first.
fn script_applied() -> PerTick {
    per_tick(&[
        &[(1, b"a")],
        &[(0, b"b"), (0, b"c")],
        &[],
        &[(0, b"g"), (0, b"h"), (1, b"f")],
        &[(0, b"m"), (0, b"n")],
    ])
}

/// What the three ticks after `chained_with_tail()`'s five apply: the inputs held for later ticks,
/// and the tail.
fn tail_applied() -> PerTick {
    per_tick(&[&[(1, b"d")], &[(0, b"k"), (1, b"j")], &[(1, b"l")]])
}

/// Inputs as `(slot, stamp, payload)`; a run of one slot's inputs within a tick is one packet.
type Sends = &'static [(u32, u64, &'static [u8])];

/// What the players send before each tick, 0 to 4, each player's stamps never back. Player 0
/// sends b, c, g, h, m, n; player 1 sends a, e, f, d, i, j. Before tick 3, e and f are one
/// packet, g and h another, d and i a third.
const SCRIPT: [Sends; 5] = [
    &[(1, 0, b"a"), (0, 1, b"b")],
    &[(0, 1, b"c")],
    &[],
    &[
        (1, 0, b"e"),
        (1, 1, b"f"),
        (0, 2, b"g"),
        (0, 2, b"h"),
        (1, 5, b"d"),
        (1, 6, b"i"),
        (0, 3, b"m"),
    ],
    &[(0, 4, b"n"), (1, 6, b"j")],
];

/// What the players send after tick 4, which stays unsealed: k applies at 6, l at 7.
const TAIL: Sends = &[(0, 6, b"k"), (1, 7, b"l")];

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
                let input = chain.extend(Tick::new(stamp), payload);
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
                .map(|&(stamp, payload)| chain.extend(Tick::new(stamp), payload))
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
            packet
                .submit(&mut log, &mut outcomes)
                .map_err(|error| LogError::Input {
                    tick: Tick::new(tick),
                    error,
                })?;
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
    let applied = (0..ticks.get()).map(|_| seal(&mut log)).collect();
    Recorded { log, applied }
}

#[test]
fn inputs_apply_by_the_delay_rule_in_slot_order() {
    let mut log = new_log();
    let mut outcomes = Vec::new();
    let mut applied = Vec::new();
    for packets in chained() {
        for packet in packets {
            packet.submit(&mut log, &mut applied).unwrap();
            let payloads = packet.inputs.iter().map(|input| input.payload);
            outcomes.extend(payloads.zip(applied.iter().copied()));
        }
        drop(log.seal_tick());
    }
    // Applied at max(stamp, next tick); late when the next tick is more than 2 ticks after the
    // stamp, early when the stamp is more than 2 ticks ahead; and a player's inputs fill each tick
    // up to 2, in chain order, the rest spilling to the next. Player 0's g, h and m arrive before
    // one tick, one past the max of 2, as a network that holds packets groups them: no stamp
    // holds more than 2, so the log takes all three, and m spills.
    assert_eq!(
        outcomes,
        [
            (&b"a"[..], Applied::At(Tick::new(0))), // stamp 0, before tick 0
            (b"b", Applied::At(Tick::new(1))),      // stamp 1 is ahead of tick 0
            (b"c", Applied::At(Tick::new(1))),      // stamp 1, before tick 1: the second of tick 1
            (b"e", Applied::Late),                  // stamp 0, before tick 3: 3 ticks late
            (b"f", Applied::At(Tick::new(3))), // stamp 1, before tick 3: 2 ticks late, the most allowed
            (b"g", Applied::At(Tick::new(3))), // stamp 2, before tick 3
            (b"h", Applied::At(Tick::new(3))), // the second of player 0's tick 3
            (b"d", Applied::At(Tick::new(5))), // stamp 5, before tick 3: 2 ticks ahead, the most allowed
            (b"i", Applied::Early),            // stamp 6, before tick 3: 3 ticks ahead
            (b"m", Applied::At(Tick::new(4))), // stamp 3, before tick 3: g and h fill it, so tick 4
            (b"n", Applied::At(Tick::new(4))), // stamp 4, before tick 4: the second of tick 4
            (b"j", Applied::At(Tick::new(6))), // stamp 6, before tick 4: the chain goes on after i
        ]
    );
    assert_eq!(log.next_tick(), Tick::new(5));

    let applied = record(header(), &chained()).unwrap().applied;
    assert_eq!(applied, script_applied());
}

#[test]
fn a_rewound_log_replays_the_same_ticks_and_ends_as_it_was() {
    let log = published();
    let bytes = encoded(&log);
    let Recorded { mut log, applied } = replayed(log);
    assert_eq!(applied, script_applied());
    // Caught up, the log is as it was: the same file, and the inputs held for later ticks, and
    // the tail, wait to apply.
    assert_eq!(encoded(&log), bytes);
    assert_eq!(
        [seal(&mut log), seal(&mut log), seal(&mut log)].to_vec(),
        tail_applied()
    );

    // A log with no sealed tick rewinds to itself.
    let empty = new_log().rewound();
    assert_eq!(empty.next_tick(), Tick::new(0));
}

/// A change to the sent packets, and the first packet it makes the log refuse.
#[derive(Debug)]
struct Tamper {
    name: &'static str,
    change: fn(&mut Vec<Vec<Sent<'static>>>),
    refused: LogError,
}

fn refused(tick: u64, error: InputError) -> LogError {
    LogError::Input {
        tick: Tick::new(tick),
        error,
    }
}

/// Sends with a broken chain or signature, or no player, each with the packet it makes the log
/// refuse.
fn broken_sends() -> [Tamper; 8] {
    [
        Tamper {
            name: "player 0's c dropped: the head after g and h is not the one signed",
            change: |ticks| drop(ticks[1].remove(0)),
            refused: refused(3, InputError::BadSignature),
        },
        Tamper {
            name: "player 0's g and h swapped in their packet",
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
                        stamp: Tick::new(2),
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
    ]
}

/// Sends past a limit of the terms, each with the packet it makes the log refuse.
fn sends_past_limits() -> [Tamper; 6] {
    [
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
            name: "player 1 sends 2 inputs of stamp 0, then 1 more of stamp 0, before tick 0",
            change: |ticks| {
                let packets = resent(1, &[&[(0, b"a"), (0, b"x")], &[(0, b"y")]], 1);
                drop(ticks[0].splice(0..1, packets));
            },
            refused: refused(0, InputError::TooManyInputs),
        },
        Tamper {
            name: "player 1 stamps its second input before its first",
            change: |ticks| ticks[0][0] = resent(1, &[&[(1, b"a"), (0, b"x")]], 1).remove(0),
            refused: refused(0, InputError::StampBack),
        },
        Tamper {
            name: "player 1 sends 7 inputs before tick 0, where ticks 0 to 2 hold 6",
            change: |ticks| {
                let sends: [&[(u64, &[u8])]; 4] = [
                    &[(0, b"a"), (0, b"x")],
                    &[(1, b"x"), (1, b"x")],
                    &[(2, b"x"), (2, b"x")],
                    &[(3, b"x")],
                ];
                drop(ticks[0].splice(0..1, resent(1, &sends, 1)));
            },
            refused: refused(0, InputError::AheadOfTime),
        },
        Tamper {
            name: "player 1's packet of 2 inputs of stamp 0 and 1 of stamp 1, one over the max",
            change: |ticks| {
                let packet = resent(1, &[&[(0, b"a"), (0, b"x"), (1, b"y")]], 1).remove(0);
                ticks[0][0] = packet;
            },
            refused: refused(0, InputError::TooManyInputs),
        },
    ]
}

#[test]
fn a_tampered_packet_is_refused() {
    for case in broken_sends().into_iter().chain(sends_past_limits()) {
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
    // The log's position bound, lowered from 2³² − 1 here: the 12th input, j, needs 12
    // inputs and 12 payload bytes.
    let bounded = |bound| {
        let mut log = new_log();
        log.position_bound = bound;
        record_into(log, &chained()).err()
    };
    let full = LogError::Input {
        tick: Tick::new(4),
        error: InputError::LogFull,
    };
    assert_eq!(bounded(11), Some(full.clone()));
    assert_eq!(bounded(12), None);
    let file = encoded(&record(header(), &chained()).unwrap().log);
    assert_eq!(SessionLog::decode_within(&file, 11).err(), Some(full));
    assert!(SessionLog::decode_within(&file, 12).is_ok());
    // The payload bytes count on their own: 1 input of 4 bytes passes a bound of 3.
    let mut log = new_log();
    log.position_bound = 3;
    let oversized = resent(1, &[&[(0, b"abcd")]], 1).remove(0);
    let mut applied = Vec::new();
    assert_eq!(
        oversized.submit(&mut log, &mut applied),
        Err(InputError::LogFull)
    );
}

#[test]
fn a_refused_packet_leaves_the_log_unchanged() {
    // The real packets still link after the refused ones, and a refused packet does not count
    // towards the max per tick.
    let sent = chained();
    let mut applied = Vec::new();
    let mut log = new_log();
    let (b, c) = (&sent[0][1], &sent[1][0]);
    let b_over_c = Sent {
        signature: c.signature,
        ..b.clone()
    };
    let three = resent(0, &[&[(1, b"b"), (1, b"x"), (1, b"y")]], 0).remove(0);
    assert_eq!(
        c.submit(&mut log, &mut applied),
        Err(InputError::BadSignature)
    );
    assert_eq!(
        b_over_c.submit(&mut log, &mut applied),
        Err(InputError::BadSignature)
    );
    assert_eq!(
        three.submit(&mut log, &mut applied),
        Err(InputError::TooManyInputs)
    );
    assert_eq!(b.submit(&mut log, &mut applied), Ok(()));
    assert_eq!(applied, [Applied::At(Tick::new(1))]);
    assert_eq!(c.submit(&mut log, &mut applied), Ok(()));
    assert_eq!(applied, [Applied::At(Tick::new(1))]);
}

#[test]
fn a_delegation_for_another_server_or_session_is_refused() {
    let cases = [
        (
            delegation_with(1, |terms| terms.server_key = x_only(42)),
            ScopeError::OtherServer,
        ),
        (
            delegation_with(1, |terms| terms.session_id = SessionId::new([32; 32])),
            ScopeError::OtherSession,
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
        |terms| terms.server_key = x_only(42),
        |terms| terms.tick_hz = NonZeroU32::new(301).unwrap(),
        |terms| terms.max_input_delay = Ticks::new(terms.max_input_delay.get() + 1),
        |terms| terms.max_input_lead = Ticks::new(terms.max_input_lead.get() + 1),
        |terms| terms.max_payload_len += 1,
        |terms| terms.max_inputs_per_tick += 1,
        |terms| terms.seed_commitment = SeedChain::new([6; 32], NonZeroU32::MIN).commitment(),
        |terms| terms.release.push('1'),
        |terms| terms.mode = Fingerprint::new([50; 32]),
        |terms| terms.dependencies[1] = Fingerprint::new([54; 32]),
        |terms| terms.dependencies.swap(0, 1),
    ];
    for (at, change) in changes.into_iter().enumerate() {
        let mut other = header();
        change(&mut other.terms);
        let error = if other.terms.server_key == x_only(41) {
            ScopeError::OtherSession
        } else {
            ScopeError::OtherServer
        };
        let refused = HeaderError::Scope {
            slot: PlayerSlot::new(0),
            error,
        };
        assert_eq!(SessionLog::new(other).err(), Some(refused), "change {at}");
    }

    // The id, as design 05 spells it.
    let mut spelled = Hasher::new();
    spelled
        .update(b"campfire/session-id/v1")
        .update(&[41; 32])
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
fn refuses(other: &SessionHeader, error: ScopeError) {
    let refused = HeaderError::Scope {
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
    let (s0, s1) = (SEED_CHAIN.seed(0), SEED_CHAIN.seed(1));

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

    let mut log = new_log();
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
    put(&mut bytes, &terms.server_key.serialize());
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
        packet.submit(&mut log, &mut applied).unwrap();
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
    assert_eq!(decoded.next_tick(), Tick::new(5));
    assert_eq!(encoded(&decoded), bytes);
    let live = record(header(), &sent).unwrap().applied;
    let decoded_again = SessionLog::decode(&bytes).unwrap();
    assert_eq!(replayed(decoded_again).applied, live);

    // What waits, waits in both logs: d applies at tick 5, k and j at 6, l at 7.
    let expected = tail_applied();
    let three = |log: &mut SessionLog| [seal(log), seal(log), seal(log)].to_vec();
    assert_eq!(three(&mut log), expected);
    assert_eq!(three(&mut decoded), expected);

    // An unpublished log, and one with no ticks, decode too.
    let empty = new_log();
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
    let mut log = new_log();
    let a = chained()[0][0].clone();
    let mut applied = Vec::new();
    a.submit(&mut log, &mut applied).unwrap();
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

/// The smallest log with every kind of byte a log file holds: the header of 2 players, 1 tick
/// with 1 packet of 2 inputs, 1 packet after the tick, and the revealed seed.
fn minimal() -> SessionLog {
    let mut log = new_log();
    let mut applied = Vec::new();
    let sent = resent(1, &[&[(0, b"a"), (0, b"b")], &[(1, b"c")]], 1);
    sent[0].submit(&mut log, &mut applied).unwrap();
    drop(log.seal_tick());
    sent[1].submit(&mut log, &mut applied).unwrap();
    log.reveal_seed(server_seed());
    log
}

#[test]
fn every_truncation_and_every_flip_of_a_log_file_is_refused() {
    // A signature, a chain link or the commitment covers every byte: the session id hashes the
    // terms, each main key signs its delegation with the contribution, each session key its
    // inputs, and the reveal checks against the commitment.
    let bytes = encoded(&minimal());
    assert_eq!(
        SessionLog::decode(&bytes).map(|log| log.next_tick()),
        Ok(Tick::new(1))
    );
    let tag = b"campfire/session-log/v1".len();
    for at in 0..bytes.len() {
        let expected = if at < tag {
            LogError::NotLog
        } else {
            LogError::Truncated
        };
        assert_eq!(
            SessionLog::decode(&bytes[..at]).err(),
            Some(expected),
            "truncated to {at} bytes"
        );
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
    let count_at = encoded(&new_log()).len() - 3;
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
    for (at, (bytes, error)) in cases.into_iter().enumerate() {
        assert_eq!(SessionLog::decode(&bytes).err(), Some(error), "case {at}");
    }
}
