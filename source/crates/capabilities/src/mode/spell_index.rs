use serde::{Deserialize, Serialize};

/// A spell of the mode, by its place among its spells, sorted by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpellIndex(u16);

impl SpellIndex {
    pub(crate) fn new(index: usize) -> SpellIndex {
        SpellIndex(u16::try_from(index).expect("a mode's spells fit u16"))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
