use serde::{Deserialize, Serialize};

/// A track of the mode, by its place among the mode's tracks in name order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrackId(u8);

impl TrackId {
    /// The most tracks a mode declares: every place a `TrackSet` holds.
    pub const LIMIT: usize = 32;

    /// The track at `index`, which is below the limit.
    pub const fn new(index: u8) -> TrackId {
        assert!(
            (index as usize) < TrackId::LIMIT,
            "a track is below the limit"
        );
        TrackId(index)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
