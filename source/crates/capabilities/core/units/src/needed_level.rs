use std::num::NonZeroU32;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// The level of a unit's `level` track that a rank of a slot's action needs, from 1: what a
/// slot kind's data names, which the capability that learns ranks compares a unit's level with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NeededLevel(NonZeroU32);

impl NeededLevel {
    /// `None` for 0.
    pub const fn new(level: u32) -> Option<Self> {
        match NonZeroU32::new(level) {
            Some(level) => Some(Self(level)),
            None => None,
        }
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Data is untrusted, so a level of 0 fails to read.
impl<'de> Deserialize<'de> for NeededLevel {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u32::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("a level is at least 1"))
    }
}
