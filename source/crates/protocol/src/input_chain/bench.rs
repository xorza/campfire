use std::hint::black_box;

use criterion::Criterion;
use secp256k1::{Keypair, Secp256k1, SecretKey};

use crate::input_chain::InputChain;
use crate::input_hash::InputHash;
use crate::session_id::SessionId;
use campfire_math::PlayerSlot;

/// The per-packet cost of the chain-head signature: the client signs the chain head once per
/// packet with its session key, and the log checks it once per packet.
pub fn chain_head_signature(c: &mut Criterion) {
    let secp = Secp256k1::new();
    let secret = SecretKey::from_byte_array(&[7; 32]).expect("a valid secret key");
    let keypair = Keypair::from_secret_key(&secp, &secret);
    let (public_key, _) = keypair.x_only_public_key();
    let session_id = SessionId::new([5; 32]);

    let mut chain = InputChain::new(PlayerSlot::new(3), InputHash::new([1; 32]));
    chain.extend(40, b"an order");
    let signature = chain.sign(&secp, &keypair, session_id, &[0; 32]);

    c.bench_function("sign_chain_head", |b| {
        b.iter(|| chain.sign(&secp, &keypair, black_box(session_id), &[0; 32]));
    });
    c.bench_function("check_chain_head", |b| {
        b.iter(|| {
            assert!(chain.signed_by(&secp, &public_key, session_id, black_box(&signature)));
        });
    });
}
