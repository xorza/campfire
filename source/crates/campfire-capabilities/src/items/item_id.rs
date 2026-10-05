use serde::{Deserialize, Serialize};

/// An item type of the mode's item book, by its place there, the order of its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(u32);

impl ItemId {
    /// The item at `index` of the book.
    pub(crate) const fn nth(index: u32) -> ItemId {
        ItemId(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}
