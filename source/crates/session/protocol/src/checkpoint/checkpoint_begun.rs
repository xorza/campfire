use campfire_common::{StateHash, Tick};

use crate::checkpoint::Checkpoint;
use crate::checkpoint::log_carry::LogCarry;
use crate::snapshot_fingerprint::SnapshotFingerprint;

/// A checkpoint the log began and has no record of yet: segment `segment` starts at the boundary
/// before tick `tick`, and the log's own state there is `carry`, which its record must carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointBegun {
    pub segment: u32,
    pub tick: Tick,
    pub carry: LogCarry,
}

impl CheckpointBegun {
    /// The checkpoint's record, of the state of hash `state_hash` at its boundary, whose snapshot
    /// has the fingerprint `snapshot`.
    pub fn record(self, state_hash: StateHash, snapshot: SnapshotFingerprint) -> Checkpoint {
        Checkpoint {
            segment: self.segment,
            tick: self.tick,
            state_hash,
            snapshot,
            carry: self.carry,
        }
    }
}
