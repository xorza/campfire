use serde::{Deserialize, Serialize};

use crate::bits256::Bits256;
use crate::names::NameId;

/// A tag, by its place in the match's list of tag names, in the order they were declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tag(u8);

impl Tag {
    /// The tag at `index` in the list of names, which is below `LIMIT`.
    pub const fn new(index: u8) -> Self {
        Self(index)
    }

    /// Its place in the list of names.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl NameId for Tag {
    type Scope = ();
    /// The most tags a match has.
    const LIMIT: usize = Bits256::BITS;

    fn at(index: usize) -> Self {
        Self(u8::try_from(index).expect("tags fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}
