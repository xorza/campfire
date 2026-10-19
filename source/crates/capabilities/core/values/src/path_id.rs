use serde::{Deserialize, Serialize};

use crate::name_kind::{Article, NameKind, NameScope, Named};
use crate::names::NameId;

/// A path of the map, by its place in the map's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PathId(u32);

impl NameId for PathId {
    type Scope = ();
    const LIMIT: usize = 1 << u32::BITS;

    fn at(index: usize) -> Self {
        Self(u32::try_from(index).expect("paths fit u32"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl Named for PathId {
    const NAMES: NameKind = NameKind::new("path", Article::A, NameScope::Match);
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::path_id::PathId;

    impl PathId {
        /// The path at `index` of the map's.
        pub const fn new(index: u32) -> Self {
            Self(index)
        }
    }
}
