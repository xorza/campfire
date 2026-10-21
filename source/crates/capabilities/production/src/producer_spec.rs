use std::num::NonZeroU8;

use campfire_common::Ticks;
use campfire_declarations::PerType;
use campfire_values::UnitType;

use crate::frame_point::FramePoint;

/// A producer type's `production` with its names resolved: the most entries its train queue
/// holds, its exit and default rally points, whether the gatherers that leave its exit start to
/// gather, the unit types that leave its exit once, as its site completes, and the spawns it
/// keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducerSpec {
    pub queue: NonZeroU8,
    pub exit: Option<FramePoint>,
    pub rally: Option<FramePoint>,
    pub gathers: bool,
    pub free: Box<[UnitType]>,
    pub spawns: Option<SpawnSpec>,
}

/// A producer's spawns as a match runs them: `count` units of `unit_type`, each that dies coming
/// back `replace` ticks later, rounded up (04-capabilities/production.md, A building may keep
/// spawns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnSpec {
    pub unit_type: UnitType,
    pub count: NonZeroU8,
    pub replace: Ticks,
}

impl PerType for ProducerSpec {
    const SCRIPTS_ONLY: bool = false;
}
