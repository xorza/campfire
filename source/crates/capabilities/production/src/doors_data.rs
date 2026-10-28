use std::num::NonZeroU8;

use serde::{Deserialize, Serialize};

/// A producer's `doors`: how many, and how long, in milliseconds, each opens, stays open after the
/// last unit through it, and closes, as Zero Hour's `ProductionUpdate` gives `NumDoorAnimations`,
/// `DoorOpeningTime`, `DoorWaitOpenTime` and `DoorCloseTime` (04-capabilities/production.md, A
/// producer's door opens before its unit leaves).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoorsData {
    pub count: NonZeroU8,
    pub opening_ms: u32,
    pub open_ms: u32,
    pub closing_ms: u32,
}
