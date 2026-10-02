use bevy_ecs::resource::Resource;

use crate::stats::pool_id::PoolId;

/// The mode's life pool, which its `[combat]` names: a unit with it lives while it holds more
/// than nothing, and is a target. Package data, not state; a match whose mode names none has no
/// life pool, and no unit is a target.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LifePool(pub(crate) PoolId);
