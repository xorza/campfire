use secp256k1::SecretKey;

use super::*;

fn root(byte: u8) -> InputHash {
    InputHash::new([byte; 32])
}

/// The head of player `slot`'s chain from `root` after `inputs`, each `(stamp, payload)`.
fn head(slot: u32, root: InputHash, inputs: &[(u64, &[u8])]) -> InputHash {
    let mut chain = InputChain::new(PlayerSlot::new(slot), root);
    for &(stamp, payload) in inputs {
        chain.extend(Tick::new(stamp), payload);
    }
    chain.head()
}

#[test]
fn the_hash_and_the_signature_cover_every_field_in_their_layout() {
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
    let keypair =
        |byte| Keypair::from_secret_key(&secp, &SecretKey::from_byte_array(&[byte; 32]).unwrap());
    let (session_key, session) = (keypair(21), SessionId::new([41; 32]));
    let mut chain = InputChain::new(PlayerSlot::new(1), root(1));
    chain.extend(Tick::new(0), b"a");
    let signature = chain.sign(&secp, &session_key, session, &[0; 32]);
    let message = [
        &b"campfire/input/v1"[..],
        session.as_bytes(),
        &1_u32.to_le_bytes(),
        &0_u64.to_le_bytes(),
        after_a.as_bytes(),
    ]
    .concat();
    let key = session_key.x_only_public_key().0;
    let schnorr = schnorr::Signature::from_byte_array(signature.to_bytes());
    assert_eq!(secp.verify_schnorr(&schnorr, &message, &key), Ok(()));

    // Another session, key or head, and the signature does not hold.
    assert!(chain.signed_by(&secp, &key, session, &signature));
    assert!(!chain.signed_by(&secp, &key, SessionId::new([32; 32]), &signature));
    let other_key = keypair(22).x_only_public_key().0;
    assert!(!chain.signed_by(&secp, &other_key, session, &signature));
    chain.extend(Tick::new(0), b"b");
    assert!(!chain.signed_by(&secp, &key, session, &signature));
}
