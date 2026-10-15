/// A package's place among a mode's packages: 0 the mode, then each package it depends on, in
/// the order of their names in its manifest. The load checks that a mode's packages fit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageIndex(u16);

impl PackageIndex {
    pub const MODE: PackageIndex = PackageIndex(0);

    /// The place of the dependency at `at` among the mode's; `None` past what an index holds.
    pub(crate) fn dependency(at: usize) -> Option<PackageIndex> {
        at.checked_add(1)
            .and_then(|index| u16::try_from(index).ok())
            .map(PackageIndex)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

/// The place `place`, as the books name a unit type's package by its place; a lookup among a
/// mode's packages checks it.
impl From<u16> for PackageIndex {
    fn from(place: u16) -> PackageIndex {
        PackageIndex(place)
    }
}

#[cfg(test)]
mod tests;
