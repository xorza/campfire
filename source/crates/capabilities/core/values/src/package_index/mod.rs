use serde::{Deserialize, Serialize};

use crate::names::MatchScope;

/// A package's place among a mode's packages: 0 the mode, then each package it depends on, in
/// the order of their names in its manifest. The load checks that a mode's packages fit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PackageIndex(u16);

/// A name scoped by its package is that package's own, as a modifier's is.
impl MatchScope for PackageIndex {
    const MATCH: Option<Self> = None;
}

impl PackageIndex {
    pub const MODE: Self = Self(0);

    /// The package at `place`; a lookup among a mode's packages checks it.
    pub const fn new(place: u16) -> Self {
        Self(place)
    }

    /// The place of the dependency at `at` among the mode's; `None` past what an index holds.
    pub fn dependency(at: usize) -> Option<Self> {
        at.checked_add(1)
            .and_then(|index| u16::try_from(index).ok())
            .map(PackageIndex)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Its place among the mode's packages.
impl From<PackageIndex> for usize {
    fn from(index: PackageIndex) -> Self {
        Self::from(index.0)
    }
}

#[cfg(test)]
mod tests;
