use serde::{Deserialize, Serialize};

use crate::names::NameId;

/// A kind of damage, by its place in the mode's `[combat] damage_kinds`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct DamageKind(u8);

impl NameId for DamageKind {
    type Scope = ();
    /// The most damage kinds a mode declares: as many as a byte tells apart.
    const LIMIT: usize = 1 << u8::BITS;

    fn at(index: usize) -> DamageKind {
        DamageKind(u8::try_from(index).expect("damage kinds fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::damage_kind::DamageKind;

    impl DamageKind {
        /// The damage kind at `index` of the mode's.
        pub const fn new(index: u8) -> DamageKind {
            DamageKind(index)
        }
    }
}
