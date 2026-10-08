use serde::{Deserialize, Serialize};

/// A path of the map, by its place in the map's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PathId(u32);

impl PathId {
    pub(crate) const fn new(index: u32) -> PathId {
        PathId(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}
