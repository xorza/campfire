use campfire_common::Tick;

use crate::checkpoint::log_carry::LogCarry;

/// A checkpoint the log began and has no record of yet: segment `segment` starts at the boundary
/// before tick `tick`, and the log's own state there is `carry`, which its record must carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointBegun {
    pub segment: u32,
    pub tick: Tick,
    pub carry: LogCarry,
}
