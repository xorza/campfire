use campfire_common::{Meaning, Value32};

/// `C(s_0)`, the hash of the first segment's server seed. The terms hold it, so the session id
/// binds the server to every segment's seed before any player joins.
pub type SeedCommitment = Value32<SeedCommitmentMeaning>;

/// The meaning of a `SeedCommitment`.
#[derive(Debug)]
pub enum SeedCommitmentMeaning {}

impl Meaning for SeedCommitmentMeaning {
    const NAME: &'static str = "SeedCommitment";
}
