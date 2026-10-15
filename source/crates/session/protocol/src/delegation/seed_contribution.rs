use campfire_common::{Meaning, Value32};

/// A player's random share of every segment's seed, chosen after the session id fixes the
/// server's seed commitment, so no one can choose it with the seed in view. Its delegation's tag
/// writes it in hex.
pub type SeedContribution = Value32<SeedContributionMeaning>;

/// The meaning of a `SeedContribution`.
#[derive(Debug)]
pub enum SeedContributionMeaning {}

impl Meaning for SeedContributionMeaning {
    const NAME: &'static str = "SeedContribution";
}
