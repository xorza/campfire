use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::items::item_data::ItemData;
use crate::values::declared_name::DeclaredName;

/// An item type of the mode's item book, by its place there, the order of its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(u32);

impl ItemId {
    /// The item type `name` of the mode's `items`; `None` when the mode does not declare it.
    pub fn named(items: &BTreeMap<DeclaredName, ItemData>, name: &str) -> Option<ItemId> {
        let at = items.keys().position(|held| held.as_str() == name)?;
        Some(ItemId(u32::try_from(at).ok()?))
    }

    pub(crate) const fn index(self) -> usize {
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
