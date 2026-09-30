use serde::{Deserialize, Serialize};

/// A path of the map, by its place in the map's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PathId(u32);

impl PathId {
    pub(crate) fn new(index: usize) -> PathId {
        PathId(u32::try_from(index).expect("paths fit u32"))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
