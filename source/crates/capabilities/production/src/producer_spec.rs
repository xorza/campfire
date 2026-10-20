use std::num::NonZeroU8;

use campfire_declarations::PerType;
use campfire_values::UnitType;

use crate::frame_point::FramePoint;

/// A producer type's `production` with its names resolved: the most entries its train queue
/// holds, its exit and default rally points, whether the gatherers that leave its exit start to
/// gather, and the unit types that leave its exit once, as its site completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducerSpec {
    pub queue: NonZeroU8,
    pub exit: Option<FramePoint>,
    pub rally: Option<FramePoint>,
    pub gathers: bool,
    pub free: Box<[UnitType]>,
}

impl PerType for ProducerSpec {
    const SCRIPTS_ONLY: bool = false;
}
