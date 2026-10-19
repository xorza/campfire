use crate::name_kind::{Article, NameKind, NameScope, Named};
use crate::names::NameId;
use serde::{Deserialize, Serialize};

/// An item type of the mode's item book, by its place there, the order of its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(u32);

impl NameId for ItemId {
    type Scope = ();
    const LIMIT: usize = 1 << u32::BITS;

    fn at(index: usize) -> Self {
        Self(u32::try_from(index).expect("items fit u32"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl Named for ItemId {
    const NAMES: NameKind = NameKind::new("item type", Article::An, NameScope::Match);
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::item_id::ItemId;

    impl ItemId {
        /// The item at `index` of the book.
        pub const fn nth(index: u32) -> Self {
            Self(index)
        }
    }
}
