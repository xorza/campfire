use serde::{Deserialize, Serialize};

/// The number of an entity's stable id, its identity in the sim, the protocol and replays, as a
/// plain value holds it outside the ECS; the sim's `StableId` converts to it and from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StableNumber(u64);

impl StableNumber {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}
