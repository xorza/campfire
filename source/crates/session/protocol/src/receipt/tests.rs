use campfire_codec::BinaryError;

use super::*;
use crate::harness::test_key::TestKey;
use crate::verifier::Verifier;

#[test]
fn a_receipt_round_trips_and_holds_only_under_its_key_over_its_head() {
    let server = TestKey::server();
    let key = server.x_only_public_key().0;
    let receipt = Receipt {
        session_id: SessionId::new([8; 32]),
        player_slot: PlayerSlot::new(1),
        delegation: DelegationId::new([2; 32]),
        tick: Tick::new(90),
        seq: 7,
        head: InputHash::new([3; 32]),
    };
    let signed = SignedReceipt {
        receipt,
        signature: TestKey::signer(server).sign(&receipt),
    };
    // A buffer that held other bytes is cleared first, so the file decodes.
    let mut bytes = vec![9; 3];
    signed.encode(&mut bytes);
    assert_eq!(SignedReceipt::decode(&bytes), Ok(signed));
    assert_eq!(
        SignedReceipt::decode(&bytes[1..]),
        Err(TaggedError::OtherTag)
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        SignedReceipt::decode(&trailing),
        Err(TaggedError::Value(BinaryError::NotCanonical))
    );
    assert!(matches!(
        SignedReceipt::decode(&bytes[..bytes.len() - 1]),
        Err(TaggedError::Value(_))
    ));
    assert!(Verifier::new().verifies(&signed.signature, &key, &receipt));
    let stranger = TestKey::of(42);
    let stranger = stranger.x_only_public_key().0;
    assert!(!Verifier::new().verifies(&signed.signature, &stranger, &receipt));
    let others = [
        Receipt {
            head: InputHash::new([4; 32]),
            ..receipt
        },
        Receipt { seq: 8, ..receipt },
        Receipt {
            delegation: DelegationId::new([5; 32]),
            ..receipt
        },
        Receipt {
            player_slot: PlayerSlot::new(0),
            ..receipt
        },
        Receipt {
            tick: Tick::new(91),
            ..receipt
        },
        Receipt {
            session_id: SessionId::new([9; 32]),
            ..receipt
        },
    ];
    for other in others {
        assert!(
            !Verifier::new().verifies(&signed.signature, &key, &other),
            "{other:?}"
        );
    }
}
