use std::num::NonZeroU32;

use super::*;
use crate::seed_chain::SeedChain;

#[test]
fn the_seeds_from_a_segment_are_the_chains_up_to_it() {
    let chain = SeedChain::new([5; 32], NonZeroU32::new(3).unwrap());
    let all = chain.seeds();
    assert_eq!(all.last(), 2);
    assert_eq!(
        [0, 1, 2].map(|segment| all.seed(segment).cloned()),
        [0, 1, 2].map(|segment| Some(chain.seed(segment)))
    );
    assert_eq!(all.seed(3).cloned(), None);
    // A chain of the lobby's length, each seed the chain's own, which hashes from the root.
    let long = SeedChain::new([7; 32], NonZeroU32::new(1024).unwrap());
    let seeds = long.seeds();
    assert_eq!(seeds.last(), 1023);
    assert!((0..1024).all(|segment| seeds.seed(segment).cloned() == Some(long.seed(segment))));
    assert_eq!(seeds.seed(1024).cloned(), None);
    assert_eq!(
        ServerSeeds::new(0, long.seed(0)).seed(0).cloned(),
        Some(long.seed(0))
    );
    let revealed = ServerSeeds::new(1, chain.seed(1));
    // The chain's seeds commit to its commitment, from any segment's; others do not.
    assert!(all.commits_to(&chain.commitment()) && revealed.commits_to(&chain.commitment()));
    let other = ServerSeeds::new(1, ServerSeed::new([4; 32]));
    assert!(!other.commits_to(&chain.commitment()));
    assert!(!all.commits_to(&long.commitment()));
    assert_eq!(revealed.seed(0).cloned(), Some(chain.seed(0)));
    assert_eq!(revealed.seed(1).cloned(), Some(chain.seed(1)));
    assert_eq!(revealed.seed(2).cloned(), None);
}
