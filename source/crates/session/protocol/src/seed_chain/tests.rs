use blake3::Hasher;

use super::*;

const CHAIN: SeedChain = SeedChain::new([5; 32], NonZeroU32::new(2).unwrap());

#[test]
fn each_seed_hashes_to_the_one_before_and_the_first_to_the_commitment() {
    let digest = |bytes: &[u8]| {
        let mut hasher = Hasher::new();
        hasher.update(b"campfire/seed-chain/v1").update(bytes);
        *hasher.finalize().as_bytes()
    };
    // s_1 is the root, s_0 its hash, and the commitment the hash of s_0.
    let (s0, s1) = (CHAIN.seed(0), CHAIN.seed(1));
    assert_eq!(s1.as_bytes(), &[5; 32]);
    assert_eq!(s0.as_bytes(), &digest(&[5; 32]));
    let commitment = CHAIN.commitment();
    assert_eq!(commitment.as_bytes(), &digest(s0.as_bytes()));
    // A seed checks for its own segment only: 1 hash leads from s_0 to the commitment, 2
    // from s_1.
    assert!(s0.check(0, &commitment));
    assert!(s1.check(1, &commitment));
    assert!(!s1.check(0, &commitment));
    assert!(!s0.check(1, &commitment));
    // A chain of one segment has its root as that segment's seed.
    let single = SeedChain::new([5; 32], NonZeroU32::MIN);
    assert_eq!(single.seed(0), s1);
    assert!(s1.check(0, &single.commitment()));
    // Neither a chain nor its seeds print their bytes.
    assert_eq!(
        format!("{CHAIN:?}"),
        "SeedChain { root: ServerSeed(Secret(..)), len: 2 }"
    );
    assert_eq!(format!("{s0:?}"), "ServerSeed(Secret(..))");
}

#[test]
#[should_panic(expected = "segment 2 is past the chain's 2 segments")]
fn a_seed_past_the_chain_is_a_bug() {
    CHAIN.seed(2);
}
