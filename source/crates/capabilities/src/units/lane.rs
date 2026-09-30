use serde::{Deserialize, Serialize};

/// A lane of the map, by its place in the map's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Lane(u32);

impl Lane {
    pub(crate) fn new(index: usize) -> Lane {
        Lane(u32::try_from(index).expect("lanes fit u32"))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
