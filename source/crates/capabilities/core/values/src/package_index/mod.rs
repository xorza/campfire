use serde::{Deserialize, Serialize};

/// A package's place among a mode's packages: 0 the mode, then each package it depends on, in
/// the order of their names in its manifest. The load checks that a mode's packages fit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PackageIndex(u16);

impl PackageIndex {
    pub const MODE: PackageIndex = PackageIndex(0);

    /// The package at `place`; a lookup among a mode's packages checks it.
    pub const fn new(place: u16) -> PackageIndex {
        PackageIndex(place)
    }

    /// The place of the dependency at `at` among the mode's; `None` past what an index holds.
    pub fn dependency(at: usize) -> Option<PackageIndex> {
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
    fn from(index: PackageIndex) -> usize {
        usize::from(index.0)
    }
}

#[cfg(test)]
mod tests;
