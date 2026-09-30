use serde::{Deserialize, Serialize};

/// A loadout entry of the mode, by its place among the mode's entries, sorted by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LoadoutIndex(u16);

impl LoadoutIndex {
    pub(crate) fn new(index: usize) -> LoadoutIndex {
        LoadoutIndex(u16::try_from(index).expect("a mode's loadout fit u16"))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
