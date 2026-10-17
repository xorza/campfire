use std::collections::VecDeque;

use campfire_common::Tick;

use crate::checkpoint::Checkpoint;
use crate::checkpoint::checkpoint_begun::CheckpointBegun;
use crate::session_log::input_schedule::Due;
use crate::session_log::log_entries::EntriesAt;
use crate::session_log::{at, offset};
use crate::signature::Signature;

/// A session's segments, in order: the first from tick 0, every other from its checkpoint, and
/// the checkpoints begun that have no record yet.
#[derive(Debug)]
pub(super) struct Segments {
    starts: Vec<SegmentStart>,
    /// The checkpoints that start the last segments, oldest first, while their records have not
    /// come; the records come in that order.
    begun: VecDeque<CheckpointBegun>,
}

/// Where a segment starts: the first tick it holds, the checkpoint it starts from, which every
/// segment but the first has once its record came, and the log's own state at its boundary, which
/// every segment but the first has.
#[derive(Debug)]
pub(super) struct SegmentStart {
    pub(super) tick: Tick,
    pub(super) checkpoint: Option<LoggedCheckpoint>,
    pub(super) boundary: Option<Boundary>,
}

/// The log's own state at a segment's boundary, beside what its checkpoint's carry records: where
/// its buffers ended, the delegation whose key signed each slot's chain's head, the inputs still
/// to apply by index, and the bytes of its file, its checkpoint's record among them once it came.
/// A load goes back to it, and a replay of a log that forgot what came before starts there.
#[derive(Debug, Clone)]
pub(super) struct Boundary {
    pub(super) entries: EntriesAt,
    pub(super) head_signers: Vec<Option<u32>>,
    /// The inputs logged before it still to apply from it on, which a replay from it starts
    /// with.
    pub(super) pending: Vec<Due>,
    pub(super) file_len: u64,
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
                boundary: None,
            }],
            begun: VecDeque::new(),
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

    /// How many checkpoints begun have no record yet.
    pub(super) fn begun_count(&self) -> usize {
        self.begun.len()
    }

    /// The oldest checkpoint begun that has no record yet, whose record comes next.
    pub(super) fn oldest_begun(&self) -> Option<&CheckpointBegun> {
        self.begun.front()
    }

    /// The checkpoint begun last that has no record yet.
    pub(super) fn latest_begun(&self) -> Option<&CheckpointBegun> {
        self.begun.back()
    }

    /// Starts a segment at `begun`'s tick, whose checkpoint is begun, the log at `boundary`
    /// there.
    pub(super) fn begin(&mut self, begun: CheckpointBegun, boundary: Boundary) {
        debug_assert_eq!(begun.segment, offset(self.starts.len()), "the next segment");
        self.starts.push(SegmentStart {
            tick: begun.tick,
            checkpoint: None,
            boundary: Some(boundary),
        });
        self.begun.push_back(begun);
    }

    /// Keeps `logged` as the record of the oldest checkpoint begun, which then has one, and adds
    /// the `bytes` it takes in the file to the file's at its boundary; the segment it starts.
    pub(super) fn record(&mut self, logged: LoggedCheckpoint, bytes: u64) -> u32 {
        let begun = self
            .begun
            .pop_front()
            .expect("a record of the checkpoint begun");
        let start = &mut self.starts[at(begun.segment)];
        start.checkpoint = Some(logged);
        let boundary = start
            .boundary
            .as_mut()
            .expect("a segment begun has its boundary");
        boundary.file_len += bytes;
        begun.segment
    }

    /// Keeps the first `count` segments, and none of the checkpoints begun after them, as a load
    /// of the last of them does.
    pub(super) fn truncate(&mut self, count: usize) {
        debug_assert!(
            self.begun.iter().all(|begun| at(begun.segment) >= count),
            "a load drops only checkpoints begun after its save"
        );
        self.starts.truncate(count);
        self.begun.clear();
    }
}
