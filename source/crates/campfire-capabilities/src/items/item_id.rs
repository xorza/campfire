use serde::{Deserialize, Serialize};

use crate::values::names::NameId;

/// An item type of the mode's item book, by its place there, the order of its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(u32);

impl ItemId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for ItemId {
    type Scope = ();
    const LIMIT: usize = 1 << u32::BITS;

    fn at(index: usize) -> ItemId {
        ItemId(u32::try_from(index).expect("items fit u32"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(test)]
mod internals {
    use crate::items::item_id::ItemId;

    impl ItemId {
        /// The item at `index` of the book.
        pub(crate) const fn nth(index: u32) -> ItemId {
            ItemId(index)
        }
    }
}
