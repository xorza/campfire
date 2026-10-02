use serde::{Deserialize, Serialize};

/// A slot kind of the mode: its place in the mode's `[[slots]]`. A unit's slots go kind after
/// kind in that order, which is how a client binds its keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SlotKind(u8);

impl SlotKind {
    pub const fn new(index: u8) -> SlotKind {
        SlotKind(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}
