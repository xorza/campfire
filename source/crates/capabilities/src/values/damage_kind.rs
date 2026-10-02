use serde::{Deserialize, Serialize};

/// A kind of damage, by its place in the mode's `[combat] damage_kinds`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub(crate) struct DamageKind(u8);

impl DamageKind {
    /// The most damage kinds a mode declares: as many as a byte tells apart.
    pub(crate) const LIMIT: usize = 1 << u8::BITS;

    pub(crate) const fn new(index: u8) -> DamageKind {
        DamageKind(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}
