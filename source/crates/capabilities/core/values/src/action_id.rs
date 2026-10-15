use serde::{Deserialize, Serialize};

use crate::names::NameId;
use crate::package_index::PackageIndex;

/// An action, by its place in the match's action book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionId(u32);

impl ActionId {
    /// The action at `index` of the book.
    pub const fn new(index: u32) -> ActionId {
        ActionId(index)
    }
}

impl NameId for ActionId {
    type Scope = PackageIndex;
    const LIMIT: usize = 1 << u32::BITS;

    fn at(index: usize) -> ActionId {
        ActionId(u32::try_from(index).expect("actions fit u32"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}
