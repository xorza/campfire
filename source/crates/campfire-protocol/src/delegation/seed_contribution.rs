use std::str::FromStr;

use campfire_common::{Bytes32, NotHex};
use derive_more::Display;
use serde::{Deserialize, Serialize};

/// A player's random share of every segment's seed, chosen after the session id fixes the
/// server's seed commitment, so no one can choose it with the seed in view. Its delegation's tag
/// writes it as 64 lowercase hex digits, which it reads back.
#[derive(
    Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SeedContribution(Bytes32);

impl SeedContribution {
    pub const fn new(bytes: [u8; 32]) -> SeedContribution {
        SeedContribution(Bytes32::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl FromStr for SeedContribution {
    type Err = NotHex;

    fn from_str(text: &str) -> Result<SeedContribution, NotHex> {
        text.parse().map(SeedContribution)
    }
}
