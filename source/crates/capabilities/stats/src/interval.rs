use campfire_common::{Tick, Ticks};
use serde::{Deserialize, Serialize};

/// A modifier's interval: every `every` ticks, the next in tick `next`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interval {
    pub every: Ticks,
    pub next: Tick,
}
