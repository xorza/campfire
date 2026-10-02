use serde::{Deserialize, Serialize};

/// A stat, by its place among the stats the mode declares, in the order of their names: what a
/// unit's stat values are kept by. One list, the stat book's, gives every id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatId(u16);

impl StatId {
    /// The stat at `index` among the mode's, within the most stats a mode declares.
    pub(crate) fn new(index: usize) -> StatId {
        StatId(u16::try_from(index).expect("stats fit u16"))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
