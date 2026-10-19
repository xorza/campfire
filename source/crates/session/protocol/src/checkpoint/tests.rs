use campfire_codec::BinaryError;
use campfire_common::Seat;
use secp256k1::XOnlyPublicKey;

use super::*;
use crate::checkpoint::log_carry::{CarriedInput, CarriedSeat};
use crate::control::Control;
use crate::delegation::seed_contribution::SeedContribution;
use crate::delegation::{Delegation, DelegationTerms};
use crate::harness::test_key::TestKey;
use crate::input_chain::InputChain;
use crate::input_hash::InputHash;
use crate::main_key::MainKey;
use crate::session_id::SessionId;
use crate::signature::Signature;
use crate::spill::Spill;
use crate::stamp_count::StampCount;
use crate::verifier::Verifier;

/// Whether `signature` is `key`'s over `record` of session `session_id`.
fn signed_by(
    record: &Checkpoint,
    key: &XOnlyPublicKey,
    session_id: SessionId,
    signature: &Signature,
) -> bool {
    Verifier::new().verifies(signature, key, &InSession { session_id, record })
}

fn delegation() -> Delegation {
    let terms = DelegationTerms {
        session_key: TestKey::of(21).x_only_public_key().0,
        server_key: TestKey::of(41).x_only_public_key().0,
        session_id: SessionId::new([8; 32]),
        seed_contribution: SeedContribution::new([6; 32]),
        expiration: 1_700_086_400,
    };
    Delegation::sign(&TestKey::signer(TestKey::of(11)), &terms, TestKey::NOW)
}

/// A record whose carry holds a seat of each controller, and an input still due.
fn record() -> Checkpoint {
    let seat = |control| CarriedSeat {
        control,
        leaver: None,
        stamps: StampCount::default(),
        spill: Spill::default(),
    };
    let mut chain = InputChain::new(Seat::new(0), InputHash::new([2; 32]));
    chain.extend(Tick::new(4), b"a");
    let player = Control::Player {
        delegation: Box::new(delegation()),
        chain,
    };
    let reserved = CarriedSeat {
        leaver: Some(MainKey::new([12; 32])),
        ..seat(Control::Reserved)
    };
    Checkpoint {
        segment: 1,
        tick: Tick::new(5),
        state_hash: StateHash::new([3; 32]),
        snapshot: SnapshotFingerprint::new([4; 32]),
        carry: LogCarry {
            seats: vec![
                seat(player),
                seat(Control::Bot),
                seat(Control::Open),
                reserved,
            ],
            pending: vec![CarriedInput {
                tick: Tick::new(6),
                seat: Seat::new(0),
                stamp: Tick::new(6),
                payload: b"b".to_vec(),
            }],
        },
    }
}

#[test]
fn a_record_round_trips_and_its_signature_holds_only_over_it() {
    let record = record();
    let mut bytes = Vec::new();
    record.encode(&mut bytes);
    bytes.push(9);
    let read = Checkpoint::take(&bytes).unwrap();
    assert_eq!(
        read,
        Taken {
            value: record.clone(),
            rest: &[9][..],
        }
    );
    let id = SessionId::new([8; 32]);
    let signature = TestKey::signer(TestKey::of(41)).sign(&InSession {
        session_id: id,
        record: &record,
    });
    let key = TestKey::of(41).x_only_public_key().0;
    assert!(signed_by(&record, &key, id, &signature));
    assert!(!signed_by(
        &record,
        &key,
        SessionId::new([9; 32]),
        &signature
    ));
    let stranger = TestKey::of(42).x_only_public_key().0;
    assert!(!signed_by(&record, &stranger, id, &signature));
    // A change to any field, the carry's included.
    let changes: [fn(&mut Checkpoint); 5] = [
        |record| record.segment = 2,
        |record| record.tick = Tick::new(6),
        |record| record.state_hash = StateHash::new([5; 32]),
        |record| record.snapshot = SnapshotFingerprint::new([5; 32]),
        |record| record.carry.pending[0].payload = b"c".to_vec(),
    ];
    for (at, change) in changes.into_iter().enumerate() {
        let mut other = record.clone();
        change(&mut other);
        assert!(!signed_by(&other, &key, id, &signature), "change {at}");
    }

    // A carried delegation that does not parse, and bytes that do not decode.
    let json = delegation().json().to_owned();
    let json_at = bytes
        .windows(json.len())
        .position(|window| window == json.as_bytes())
        .unwrap();
    let mut broken = bytes.clone();
    broken[json_at] = b'[';
    assert!(matches!(
        Checkpoint::take(&broken),
        Err(CheckpointDecodeError::Delegation { seat, .. }) if seat == Seat::new(0)
    ));
    assert!(matches!(
        Checkpoint::take(&bytes[..10]),
        Err(CheckpointDecodeError::Malformed(BinaryError::Truncated))
    ));
}
