use std::hint::black_box;

use campfire_common::{PlayerSlot, Tick};
use criterion::Criterion;
use secp256k1::Secp256k1;

use crate::harness::test_key::TestKey;
use crate::input_chain::InputChain;
use crate::input_hash::InputHash;
use crate::session_id::SessionId;

/// The per-packet cost of the chain-head signature: the client signs the chain head once per
/// packet with its session key, and the log checks it once per packet.
pub(crate) fn chain_head(c: &mut Criterion) {
    let secp = Secp256k1::new();
    let keypair = TestKey::of(7);
    let (public_key, _) = keypair.x_only_public_key();
    let session_id = SessionId::new([5; 32]);

    let mut chain = InputChain::new(PlayerSlot::new(3), InputHash::new([1; 32]));
    chain.extend(Tick::new(40), b"an order");
    let signature = chain.sign(&secp, &keypair, session_id, &[0; 32]);

    let mut group = c.benchmark_group("atomic/chain_head");
    group.bench_function("sign", |b| {
        b.iter(|| chain.sign(&secp, &keypair, black_box(session_id), &[0; 32]));
    });
    group.bench_function("check", |b| {
        b.iter(|| {
            assert!(chain.signed_by(&secp, &public_key, session_id, black_box(&signature)));
        });
    });
    group.finish();
}
