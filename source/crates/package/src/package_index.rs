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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_is_0_and_the_last_dependency_is_the_largest_index() {
        assert_eq!(PackageIndex::MODE.get(), 0);
        assert_eq!(PackageIndex::dependency(0).map(PackageIndex::get), Some(1));
        let last = usize::from(u16::MAX) - 1;
        assert_eq!(
            PackageIndex::dependency(last).map(PackageIndex::get),
            Some(u16::MAX)
        );
        assert_eq!(PackageIndex::dependency(last + 1), None);
        assert_eq!(PackageIndex::dependency(usize::MAX), None);
    }
}
