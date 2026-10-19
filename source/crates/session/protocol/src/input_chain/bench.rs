use std::hint::black_box;

use campfire_common::{Seat, Tick};
use campfire_measure::{Kind, Scale};
use criterion::Criterion;

use crate::entropy::Entropy;
use crate::harness::test_key::TestKey;
use crate::input_chain::InputChain;
use crate::input_hash::InputHash;
use crate::session_id::SessionId;
use crate::signer::Signer;
use crate::verifier::Verifier;

/// The per-packet cost of the chain-head signature: the client signs the chain head once per
/// packet with its session key, and the log checks it once per packet.
pub(crate) fn chain_head(c: &mut Criterion) {
    let signer = Signer::new(TestKey::of(7), Entropy::Fixed(0));
    let mut verifier = Verifier::new();
    let mut scratch = Vec::new();
    let public_key = signer.public_key();
    let session_id = SessionId::new([5; 32]);

    let mut chain = InputChain::new(Seat::new(3), InputHash::new([1; 32]));
    chain.extend(Tick::new(40), b"an order");
    let signature = signer.sign(&chain.signed_head(session_id));

    let mut group = Kind::Atomic.group(c, "chain_head", Scale::Sample);
    group.bench_function("sign", |b| {
        b.iter(|| signer.sign_with(&chain.signed_head(black_box(session_id)), &mut scratch));
    });
    group.bench_function("check", |b| {
        b.iter(|| {
            let head = chain.signed_head(session_id);
            assert!(verifier.verifies(black_box(&signature), &public_key, &head));
        });
    });
    group.finish();
}
