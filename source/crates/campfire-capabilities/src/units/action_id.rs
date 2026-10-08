use serde::{Deserialize, Serialize};

/// An action, by its place in the match's action book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionId(u32);

impl ActionId {
    /// The action at `index` of the book.
    pub(crate) const fn new(index: u32) -> ActionId {
        ActionId(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}
