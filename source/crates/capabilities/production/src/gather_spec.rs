use std::num::NonZeroU32;

use campfire_actions::PerAction;
use campfire_math::Num;
use campfire_values::ResourceId;

/// What a gather action runs: the player resource it gathers, the most a trip carries, and how
/// far from its node it looks for another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatherSpec {
    pub resource: ResourceId,
    pub take: NonZeroU32,
    pub bounce: Num,
}

impl PerAction for GatherSpec {
    const SCRIPTS_ONLY: bool = false;
}
