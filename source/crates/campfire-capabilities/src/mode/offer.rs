use serde::{Deserialize, Serialize};

/// A value a choice offers: its place among the avatars the mode depends on, or among the
/// entries of its loadout packages, by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Offer(u16);

impl Offer {
    pub(crate) fn new(index: usize) -> Offer {
        Offer(u16::try_from(index).expect("a mode's offers fit u16"))
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}
