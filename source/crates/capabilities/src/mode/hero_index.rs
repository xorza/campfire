use serde::{Deserialize, Serialize};

/// A hero of the mode, by its place among the heroes the mode depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HeroIndex(u16);

impl HeroIndex {
    pub(crate) fn new(index: usize) -> HeroIndex {
        HeroIndex(u16::try_from(index).expect("a mode's heroes fit u16"))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
