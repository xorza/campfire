use campfire_sim::{StableId, Tick};
use serde::{Deserialize, Serialize};

/// The last tick `source` struck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentAttack {
    pub source: StableId,
    pub tick: Tick,
}
