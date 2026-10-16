use serde::{Deserialize, Serialize};

use crate::names::NameId;

/// A track of the mode, by its place among the mode's tracks in name order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrackId(u8);

impl TrackId {
    /// Its place in the mode's tracks.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl NameId for TrackId {
    type Scope = ();
    /// The most tracks a mode declares: every place a `TrackSet` holds.
    const LIMIT: usize = 32;

    fn at(index: usize) -> Self {
        assert!(index < Self::LIMIT, "a track is below the limit");
        Self(u8::try_from(index).expect("tracks fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::names::NameId;
    use crate::track_id::TrackId;

    impl TrackId {
        /// The track at `index`, which is below the limit.
        pub const fn new(index: u8) -> Self {
            assert!(
                (index as usize) < <Self as NameId>::LIMIT,
                "a track is below the limit"
            );
            Self(index)
        }
    }
}
