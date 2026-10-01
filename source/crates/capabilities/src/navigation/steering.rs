use campfire_sim::Position;

use crate::navigation::body_index::{BodyIndex, IndexedBody};

/// What steering keeps between ticks: the index of the units that stand, made on the first tick
/// with the static index's buckets, and the buffers each tick refills, so a tick allocates
/// nothing once they have grown.
#[derive(Debug, Default)]
pub(crate) struct Steering {
    pub(crate) standing: Option<BodyIndex>,
    /// The bodies of the units that stand, and of those that walk, by stable id.
    pub(crate) still: Vec<IndexedBody>,
    pub(crate) walking: Vec<IndexedBody>,
    /// The units that block the walker that steers now.
    pub(crate) blockers: Vec<IndexedBody>,
    /// The short route of the walker that steers now.
    pub(crate) short: Vec<Position>,
}

impl Steering {
    /// The cells a short route's window reaches from the walker's along each axis: room to go
    /// round a unit some cells wide on either side, in a search of at most 17 × 17 cells.
    pub(crate) const WINDOW: usize = 8;
    /// How long a walker moves less than half a step a tick before it marks the walkers it
    /// touches: long enough that a walker yielding for a moment keeps its way.
    pub(crate) const STUCK_MS: u64 = 150;
}
