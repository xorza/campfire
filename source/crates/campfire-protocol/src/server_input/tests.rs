use super::*;
use crate::delegation::DelegationTerms;
use crate::delegation::seed_contribution::SeedContribution;
use crate::harness::test_key::TestKey;

/// A delegation of the main key of `main` for the session key of `byte`.
fn delegation(main: u8, byte: u8) -> Delegation {
    let terms = DelegationTerms {
        session_key: TestKey::of(byte).x_only_public_key().0,
        server_key: TestKey::of(41).x_only_public_key().0,
        session_id: SessionId::new([8; 32]),
        seed_contribution: SeedContribution::new([6; 32]),
        expiration: 1_700_086_400,
    };
    Delegation::sign(
        &Secp256k1::new(),
        &TestKey::of(main),
        &terms,
        1_700_000_000,
        &[0; 32],
    )
}

#[test]
fn a_server_input_round_trips_and_its_signature_holds_only_at_its_place() {
    let slot = PlayerSlot::new(2);
    let inputs = [
        ServerInput::Bot {
            slot,
            payload: &[1, 2, 3],
        },
        ServerInput::Join {
            slot,
            delegation: delegation(11, 21),
        },
        ServerInput::Renew {
            slot,
            delegation: delegation(11, 22),
        },
        ServerInput::Leave {
            slot,
            reason: LeaveReason::Grace,
            becomes: AfterLeave::Bot,
        },
        ServerInput::Connected { slot },
        ServerInput::Disconnected { slot },
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

    let secp = Secp256k1::new();
    let server = TestKey::of(41);
    let id = SessionId::new([8; 32]);
    let place = InputPlace {
        tick: Tick::new(7),
        index: 1,
    };
    let input = &inputs[0];
    let signature = input.sign(&secp, &server, id, place, &[0; 32]);
    let key = server.x_only_public_key().0;
    assert!(input.signed_by(&secp, &key, id, place, &signature, &mut Vec::new()));
    // At another tick or index, in another session, under another key, or over another input.
    let other_tick = InputPlace {
        tick: Tick::new(8),
        ..place
    };
    let other_index = InputPlace { index: 0, ..place };
    assert!(!input.signed_by(&secp, &key, id, other_tick, &signature, &mut Vec::new()));
    assert!(!input.signed_by(&secp, &key, id, other_index, &signature, &mut Vec::new()));
    assert!(!input.signed_by(
        &secp,
        &key,
        SessionId::new([9; 32]),
        place,
        &signature,
        &mut Vec::new()
    ));
    let stranger = TestKey::of(42).x_only_public_key().0;
    assert!(!input.signed_by(&secp, &stranger, id, place, &signature, &mut Vec::new()));
    assert!(!inputs[4].signed_by(&secp, &key, id, place, &signature, &mut Vec::new()));

    // A delegation that does not parse, and bytes that do not decode.
    let mut bytes = Vec::new();
    postcard::to_io(
        &Wire::Join {
            slot: 0,
            delegation: "{}",
        },
        &mut bytes,
    )
    .unwrap();
    assert!(matches!(
        ServerInput::take(&bytes),
        Err(ServerInputDecodeError::Delegation(_))
    ));
    assert!(matches!(
        ServerInput::take(&[200]),
        Err(ServerInputDecodeError::Malformed(_))
    ));
}
