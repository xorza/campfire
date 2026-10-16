use campfire_common::Tick;

use crate::checkpoint::Checkpoint;
use crate::checkpoint::checkpoint_begun::CheckpointBegun;
use crate::session_log::{at, offset};
use crate::signature::Signature;

/// A session's segments, in order: the first from tick 0, every other from its checkpoint, and
/// the checkpoint begun that has no record yet.
#[derive(Debug)]
pub(super) struct Segments {
    starts: Vec<SegmentStart>,
    /// The checkpoint that starts the last segment, while its record has not come.
    begun: Option<CheckpointBegun>,
}

/// Where a segment starts: the first tick it holds, and the checkpoint it starts from, which
/// every segment but the first has once its record came.
#[derive(Debug)]
pub(super) struct SegmentStart {
    pub(super) tick: Tick,
    pub(super) checkpoint: Option<LoggedCheckpoint>,
}

#[derive(Debug)]
pub(super) struct LoggedCheckpoint {
    pub(super) record: Checkpoint,
    pub(super) signature: Signature,
}

impl Default for Segments {
    /// The first segment, from tick 0.
    fn default() -> Self {
        Self {
            starts: vec![SegmentStart {
                tick: Tick::new(0),
                checkpoint: None,
            }],
            begun: None,
        }
    }
}

impl Segments {
    /// How many segments the session has.
    pub(super) const fn len(&self) -> usize {
        self.starts.len()
    }

    /// The last segment, which the next tick runs in.
    pub(super) fn last(&self) -> u32 {
        offset(self.starts.len() - 1)
    }

    /// Where the last segment starts.
    pub(super) fn last_start(&self) -> &SegmentStart {
        self.starts.last().expect("a session has a segment")
    }

    /// The start of segment `segment`.
    pub(super) fn get(&self, segment: u32) -> Option<&SegmentStart> {
        self.starts.get(at(segment))
    }

    /// The first `count` segments' starts.
    pub(super) fn first(&self, count: usize) -> &[SegmentStart] {
        &self.starts[..count]
    }

    /// The segment that starts at `tick`, its record come or not; none at tick 0, where the
    /// first starts from no checkpoint.
    pub(super) fn starting(&self, tick: Tick) -> Option<u32> {
        let at = self.starts.partition_point(|segment| segment.tick < tick);
        self.starts
            .get(at)
            .filter(|segment| at > 0 && segment.tick == tick)?;
        Some(offset(at))
    }

    /// Every checkpoint record and its signature, by segment from the second.
    pub(super) fn checkpoints(&self) -> impl Iterator<Item = &LoggedCheckpoint> {
        self.starts
            .iter()
            .filter_map(|segment| segment.checkpoint.as_ref())
    }

    /// The checkpoint begun that has no record yet.
    pub(super) const fn begun(&self) -> Option<&CheckpointBegun> {
        self.begun.as_ref()
    }

    /// Starts a segment at `begun`'s tick, whose checkpoint is begun.
    pub(super) fn begin(&mut self, begun: CheckpointBegun) {
        debug_assert!(self.begun.is_none(), "one checkpoint is begun at a time");
        debug_assert_eq!(begun.segment, offset(self.starts.len()), "the next segment");
        self.starts.push(SegmentStart {
            tick: begun.tick,
            checkpoint: None,
        });
        self.begun = Some(begun);
    }

    /// Keeps `logged` as the record of the checkpoint begun, which then has one.
    pub(super) fn record(&mut self, logged: LoggedCheckpoint) {
        let begun = self.begun.take().expect("a record of the checkpoint begun");
        self.starts[at(begun.segment)].checkpoint = Some(logged);
    }
}
