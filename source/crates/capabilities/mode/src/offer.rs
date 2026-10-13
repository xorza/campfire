use campfire_values::NameId;
use serde::{Deserialize, Serialize};

/// A value a choice offers: its place among the avatars the mode depends on, or among the
/// entries of its loadout packages, by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Offer(u16);

impl Offer {
    pub fn new(index: usize) -> Offer {
        Offer(u16::try_from(index).expect("a mode's offers fit u16"))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for Offer {
    type Scope = ();
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> Offer {
        Offer::new(index)
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}
