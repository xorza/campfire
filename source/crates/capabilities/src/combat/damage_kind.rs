/// A kind of damage, by its place in the mode's `[combat] damage_kinds`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DamageKind(u8);

impl DamageKind {
    pub(crate) const fn new(index: u8) -> DamageKind {
        DamageKind(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}
