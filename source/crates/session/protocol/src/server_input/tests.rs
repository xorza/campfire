use super::*;
use crate::delegation::DelegationTerms;
use crate::delegation::seed_contribution::SeedContribution;
use crate::harness::test_key::TestKey;
use crate::signed::write_message;
use crate::verifier::Verifier;

/// A delegation of the main key of `main` for the session key of `byte`.
fn delegation(main: u8, byte: u8) -> Delegation {
    let terms = DelegationTerms {
        session_key: TestKey::of(byte).x_only_public_key().0,
        server_key: TestKey::of(41).x_only_public_key().0,
        session_id: SessionId::new([8; 32]),
        seed_contribution: SeedContribution::new([6; 32]),
        expiration: 1_700_086_400,
    };
    Delegation::sign(&TestKey::signer(TestKey::of(main)), &terms, TestKey::NOW)
}

#[test]
fn a_server_input_round_trips_and_its_signature_holds_only_at_its_place() {
    let seat = Seat::new(2);
    let inputs = [
        ServerInput::Bot {
            seat,
            payload: &[1, 2, 3],
        },
        ServerInput::Join {
            seat,
            delegation: delegation(11, 21),
        },
        ServerInput::Renew {
            seat,
            delegation: delegation(11, 22),
        },
        ServerInput::Leave {
            seat,
            reason: LeaveReason::Grace,
            becomes: AfterLeave::Bot,
        },
        ServerInput::Connected { seat },
        ServerInput::Disconnected { seat },
    ];
    for input in &inputs {
        let mut bytes = Vec::new();
        input.encode(&mut bytes);
        bytes.push(9);
        let read = ServerInput::take(&bytes).unwrap();
        assert_eq!(
            read,
            Decoded {
                value: input.clone(),
                rest: &[9][..],
            }
        );
    }
    let server = TestKey::of(41);
    let id = SessionId::new([8; 32]);
    let place = InputPlace {
        tick: Tick::new(7),
        index: 1,
    };
    let input = &inputs[0];
    let placed = |input, session_id, place| PlacedInput {
        input,
        session_id,
        place,
    };
    let signature = TestKey::signer(server).sign(&placed(input, id, place));
    let key = server.x_only_public_key().0;
    let signed_by = |input, key, session_id, place, signature| {
        Verifier::new().verifies(signature, key, &placed(input, session_id, place))
    };
    assert!(signed_by(input, &key, id, place, &signature));
    // Each input's message is the message of its encoding at its place, which a log signs once
    // it wrote the encoding.
    for input in &inputs {
        let mut encoded = Vec::new();
        input.encode(&mut encoded);
        let [mut whole, mut written] = [Vec::new(), Vec::new()];
        write_message(&placed(input, id, place), &mut whole);
        let of_encoding = EncodedPlace {
            session_id: id,
            place,
            input: &encoded,
        };
        write_message(&of_encoding, &mut written);
        assert_eq!(whole, written);
    }
    // At another tick or index, in another session, under another key, or over another input.
    let other_tick = InputPlace {
        tick: Tick::new(8),
        ..place
    };
    let other_index = InputPlace { index: 0, ..place };
    assert!(!signed_by(input, &key, id, other_tick, &signature));
    assert!(!signed_by(input, &key, id, other_index, &signature));
    assert!(!signed_by(
        input,
        &key,
        SessionId::new([9; 32]),
        place,
        &signature
    ));
    let stranger = TestKey::of(42).x_only_public_key().0;
    assert!(!signed_by(input, &stranger, id, place, &signature));
    assert!(!signed_by(&inputs[4], &key, id, place, &signature));

    // A delegation that does not parse, and bytes that do not decode.
    let bytes = Binary::encode(&Wire::Join {
        seat: 0,
        delegation: "{}",
    });
    assert!(matches!(
        ServerInput::take(&bytes),
        Err(ServerInputDecodeError::Delegation(_))
    ));
    assert!(matches!(
        ServerInput::take(&[200]),
        Err(ServerInputDecodeError::Malformed(_))
    ));
}
