use serde::{Deserialize, Serialize};

use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::name_script_arg;
use crate::values::names::NameId;

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

impl NameId for TrackId {
    type Scope = ();
    const LIMIT: usize = TrackId::LIMIT;

    fn at(index: usize) -> TrackId {
        TrackId::new(u8::try_from(index).expect("tracks fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

name_script_arg!(TrackId, NameKind::Track, |ctx, name| {
    ctx.view().names().tracks.named(name)
});
