use std::collections::VecDeque;

use campfire_common::Tick;

use crate::checkpoint::Checkpoint;
use crate::checkpoint::checkpoint_begun::CheckpointBegun;
use crate::session_log::input_schedule::InputOrder;
use crate::session_log::log_entries::EntriesAt;
use crate::session_log::{at, offset};
use crate::signature::Signature;

/// A session's segments, in order: the first from tick 0, every other from its checkpoint, and
/// the checkpoints begun that have no record yet. A log whose file starts at a checkpoint holds
/// the segments from that one's on.
#[derive(Debug)]
pub(super) struct Segments {
    /// The number of the first segment it holds.
    first: u32,
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
/// its buffers ended, the delegation whose key signed each player slot's chain's head, the inputs
/// still to apply by index, and the bytes of its file, its checkpoint's record among them once it
/// came. A load goes back to it, and a replay of a log that forgot what came before starts there.
#[derive(Debug, Clone)]
pub(super) struct Boundary {
    pub(super) entries: EntriesAt,
    pub(super) head_signers: Vec<Option<u32>>,
    /// The inputs logged before it still to apply from it on, which a replay from it starts
    /// with.
    pub(super) pending: Vec<InputOrder>,
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
            first: 0,
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
    /// The segments from `segment`'s on, which starts at `start`, as a log file that starts at
    /// its checkpoint holds them.
    pub(super) fn from_checkpoint(segment: u32, start: SegmentStart) -> Self {
        debug_assert!(
            segment > 0 && start.checkpoint.is_some() && start.boundary.is_some(),
            "a file starts at a checkpoint's segment with its record"
        );
        Self {
            first: segment,
            starts: vec![start],
            begun: VecDeque::new(),
        }
    }

    /// The first segment it holds: 0, or the one whose checkpoint its log's file starts at.
    pub(super) const fn first(&self) -> u32 {
        self.first
    }

    /// The segment a checkpoint begun next starts.
    pub(super) fn next(&self) -> u32 {
        self.first + offset(self.starts.len())
    }

    /// The last segment, which the next tick runs in.
    pub(super) fn last(&self) -> u32 {
        self.next() - 1
    }

    /// Where the last segment starts.
    pub(super) fn last_start(&self) -> &SegmentStart {
        self.starts.last().expect("a session has a segment")
    }

    /// The start of segment `segment`; none for one it does not hold.
    pub(super) fn get(&self, segment: u32) -> Option<&SegmentStart> {
        self.starts.get(at(segment.checked_sub(self.first)?))
    }

    /// The starts of the segments from `segment` on, each with its number.
    pub(super) fn since(&self, segment: u32) -> impl Iterator<Item = (u32, &SegmentStart)> {
        let skip = at(segment - self.first);
        (segment..).zip(&self.starts[skip..])
    }

    /// The segment that starts at `tick` from a checkpoint, its record come or not; none at
    /// tick 0, where segment 0 starts from none.
    pub(super) fn starting(&self, tick: Tick) -> Option<u32> {
        let at = self.starts.partition_point(|segment| segment.tick < tick);
        self.starts
            .get(at)
            .filter(|segment| segment.tick == tick && segment.boundary.is_some())?;
        Some(self.first + offset(at))
    }

    /// Every checkpoint record it holds and its signature, by segment.
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
        debug_assert_eq!(begun.segment, self.next(), "the next segment");
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
        let start = &mut self.starts[at(begun.segment - self.first)];
        start.checkpoint = Some(logged);
        let boundary = start
            .boundary
            .as_mut()
            .expect("a segment begun has its boundary");
        boundary.file_len += bytes;
        begun.segment
    }

    /// Keeps the segments up to `segment`, and none of the checkpoints begun after them, as a
    /// load of `segment` does.
    pub(super) fn truncate_after(&mut self, segment: u32) {
        debug_assert!(
            self.begun.iter().all(|begun| begun.segment > segment),
            "a load drops only checkpoints begun after its save"
        );
        self.starts.truncate(at(segment - self.first) + 1);
        self.begun.clear();
    }
}
