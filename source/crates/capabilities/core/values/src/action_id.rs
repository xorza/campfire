use serde::{Deserialize, Serialize};

use crate::name_kind::{Article, NameKind, NameScope, Named};
use crate::names::NameId;
use crate::package_index::PackageIndex;

/// An action, by its place in the match's action book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionId(u32);

impl NameId for ActionId {
    type Scope = PackageIndex;
    const LIMIT: usize = 1 << u32::BITS;

    fn at(index: usize) -> Self {
        Self(u32::try_from(index).expect("actions fit u32"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl Named for ActionId {
    const NAMES: NameKind = NameKind::new("action", Article::An, NameScope::Package);
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::action_id::ActionId;

    impl ActionId {
        /// The action at `index` of the book.
        pub const fn new(index: u32) -> Self {
            Self(index)
        }
    }
}
