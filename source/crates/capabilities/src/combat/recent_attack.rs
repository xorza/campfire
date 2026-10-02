use campfire_math::Tick;
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

/// The last tick `source` struck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentAttack {
    pub source: StableId,
    pub tick: Tick,
}
