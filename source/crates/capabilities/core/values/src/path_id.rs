use serde::{Deserialize, Serialize};

use crate::names::NameId;

/// A path of the map, by its place in the map's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PathId(u32);

impl PathId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for PathId {
    type Scope = ();
    const LIMIT: usize = 1 << u32::BITS;

    fn at(index: usize) -> PathId {
        PathId(u32::try_from(index).expect("paths fit u32"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::path_id::PathId;

    impl PathId {
        /// The path at `index` of the map's.
        pub const fn new(index: u32) -> PathId {
            PathId(index)
        }
    }
}
