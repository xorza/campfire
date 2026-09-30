use std::hint::black_box;

use criterion::Criterion;
use secp256k1::{Keypair, Secp256k1, SecretKey};

use crate::input_chain::InputChain;
use crate::input_hash::InputHash;
use crate::player_slot::PlayerSlot;

const SIGNATURE_DOMAIN: &[u8] = b"campfire/input/v1";
/// The header does not carry a session id yet; any 32 bytes cost the same to sign.
const SESSION_ID: [u8; 32] = [5; 32];

/// The per-packet cost of the chain-head signature: the client signs `domain ‖ session id ‖ u32
/// slot ‖ u64 seq ‖ chain head` once per packet with its session key, BIP-340 Schnorr, and the
/// server checks it once per packet.
pub fn chain_head_signature(c: &mut Criterion) {
    let secp = Secp256k1::new();
    let secret = SecretKey::from_byte_array(&[7; 32]).expect("a valid secret key");
    let keypair = Keypair::from_secret_key(&secp, &secret);
    let (public_key, _) = keypair.x_only_public_key();

    let mut chain = InputChain::new(PlayerSlot::new(3), InputHash::new([1; 32]));
    let last = chain.extend(40, b"an order");
    let mut message = Vec::new();
    message.extend_from_slice(SIGNATURE_DOMAIN);
    message.extend_from_slice(&SESSION_ID);
    message.extend_from_slice(&last.slot.get().to_le_bytes());
    message.extend_from_slice(&last.seq.to_le_bytes());
    message.extend_from_slice(chain.head.as_bytes());
    let signature = secp.sign_schnorr_no_aux_rand(&message, &keypair);

    c.bench_function("sign_chain_head", |b| {
        b.iter(|| secp.sign_schnorr_no_aux_rand(black_box(&message), &keypair));
    });
    c.bench_function("check_chain_head", |b| {
        b.iter(|| {
            secp.verify_schnorr(black_box(&signature), black_box(&message), &public_key)
                .expect("the signature holds");
        });
    });
}
