use serde::{Deserialize, Serialize};

use crate::bits256::Bits256;
use crate::names::NameId;

/// A tag, by its place in the match's list of tag names, in the order they were declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tag(u8);

impl Tag {
    /// The most tags a match has.
    pub const LIMIT: usize = Bits256::BITS;

    /// The tag at `index` in the list of names, which is below `LIMIT`.
    pub const fn new(index: u8) -> Tag {
        Tag(index)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for Tag {
    type Scope = ();
    const LIMIT: usize = Tag::LIMIT;

    fn at(index: usize) -> Tag {
        Tag(u8::try_from(index).expect("tags fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}
