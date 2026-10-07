use campfire_math::Num;

use crate::players::resource_id::ResourceId;

/// What a gather action runs: the player resource it gathers, the most a trip carries, and how
/// far from its node it looks for another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GatherSpec {
    pub(crate) resource: ResourceId,
    pub(crate) take: u32,
    pub(crate) bounce: Num,
}
