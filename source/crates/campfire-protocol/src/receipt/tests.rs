use secp256k1::SecretKey;

use super::*;

#[test]
fn a_receipt_round_trips_and_holds_only_under_its_key_over_its_head() {
    let secp = Secp256k1::new();
    let server = Keypair::from_secret_key(&secp, &SecretKey::from_byte_array(&[41; 32]).unwrap());
    let key = server.x_only_public_key().0;
    let receipt = Receipt {
        session_id: SessionId::new([8; 32]),
        slot: PlayerSlot::new(1),
        delegation: DelegationId::new([2; 32]),
        tick: Tick::new(90),
        seq: 7,
        head: InputHash::new([3; 32]),
    };
    let signed = SignedReceipt {
        receipt,
        signature: receipt.sign(&secp, &server, &[0; 32]),
    };
    let bytes = signed.encode();
    assert_eq!(SignedReceipt::decode(&bytes), Ok(signed));
    assert_eq!(
        SignedReceipt::decode(&bytes[1..]),
        Err(ReceiptFileError::NotReceipt)
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        SignedReceipt::decode(&trailing),
        Err(ReceiptFileError::Trailing)
    );
    assert!(matches!(
        SignedReceipt::decode(&bytes[..bytes.len() - 1]),
        Err(ReceiptFileError::Malformed(_))
    ));
    assert!(receipt.signed_by(&secp, &key, &signed.signature));
    let stranger = Keypair::from_secret_key(&secp, &SecretKey::from_byte_array(&[42; 32]).unwrap());
    assert!(!receipt.signed_by(&secp, &stranger.x_only_public_key().0, &signed.signature));
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
            slot: PlayerSlot::new(0),
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
            !other.signed_by(&secp, &key, &signed.signature),
            "{other:?}"
        );
    }
}
