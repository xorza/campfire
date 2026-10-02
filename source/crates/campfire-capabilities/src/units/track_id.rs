use serde::{Deserialize, Serialize};

/// A track of the mode, by its place among the mode's tracks in name order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrackId(u8);

impl TrackId {
    /// The most tracks a mode declares: every place a `TrackSet` holds.
    pub const LIMIT: usize = 32;

    /// The track at `index`; `None` past the limit.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "an index below the limit of 32 fits u8"
    )]
    pub const fn new(index: usize) -> Option<TrackId> {
        if index >= TrackId::LIMIT {
            return None;
        }
        Some(TrackId(index as u8))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
