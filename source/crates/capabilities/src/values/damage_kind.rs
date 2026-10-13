use serde::{Deserialize, Serialize};

use crate::values::names::NameId;

/// A kind of damage, by its place in the mode's `[combat] damage_kinds`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct DamageKind(u8);

impl DamageKind {
    /// The most damage kinds a mode declares: as many as a byte tells apart.
    pub(crate) const LIMIT: usize = 1 << u8::BITS;
}

impl NameId for DamageKind {
    type Scope = ();
    const LIMIT: usize = DamageKind::LIMIT;

    fn at(index: usize) -> DamageKind {
        DamageKind(u8::try_from(index).expect("damage kinds fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::values::damage_kind::DamageKind;

    impl DamageKind {
        /// The damage kind at `index` of the mode's.
        pub(crate) const fn new(index: u8) -> DamageKind {
            DamageKind(index)
        }
    }
}
