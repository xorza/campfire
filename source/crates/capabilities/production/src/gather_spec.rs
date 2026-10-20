use std::num::NonZeroU32;

use campfire_actions::PerAction;
use campfire_common::Ticks;
use campfire_math::Num;
use campfire_values::ResourceId;

/// What a gather action runs: the player resource it gathers, the most a trip carries, what each
/// windup takes of it, at most `take`, how far from its node it looks for another, and how long
/// its worker stands at a drop-off before its load joins its player's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatherSpec {
    pub resource: ResourceId,
    pub take: NonZeroU32,
    pub unit: NonZeroU32,
    pub bounce: Num,
    pub deliver: Ticks,
}

impl PerAction for GatherSpec {
    const SCRIPTS_ONLY: bool = false;
}
