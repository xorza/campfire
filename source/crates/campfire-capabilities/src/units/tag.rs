use serde::{Deserialize, Serialize};

use crate::units::bits256::Bits256;

/// A tag, by its place in the match's list of tag names, in the order they were declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Tag(u8);

impl Tag {
    /// The most tags a match has.
    pub(crate) const LIMIT: usize = Bits256::BITS;

    /// The tag at `index` in the list of names, which is below `LIMIT`.
    pub(crate) const fn new(index: u8) -> Tag {
        Tag(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}
