use bevy_ecs::entity::Entity;
use campfire_common::Ticks;
use campfire_math::Num;
use campfire_sim::Position;

use crate::stats::pool_cost::PoolCost;
use crate::units::unit_type::UnitType;
use crate::values::rank::Rank;

/// What a builder does with its build order this tick.
#[derive(Debug, Clone, Copy)]
pub(super) enum Step {
    /// It walks to the point of its target's box nearest it.
    Walk(Position),
    /// It stays: a builder of its site, in range.
    Build,
    /// It takes its `builder` site, of this entity, which no other builder holds, and builds it.
    Take(Entity),
    /// Its order ends.
    End,
    /// Its build starts, its site spawning.
    Start(Start),
}

/// A build that starts: the building's type and place, and what it pays and spends.
#[derive(Debug, Clone, Copy)]
pub(super) struct Start {
    pub(super) unit_type: UnitType,
    pub(super) at: Position,
    pub(super) angle: Num,
    pub(super) cost: PoolCost,
    pub(super) cooldown: Ticks,
    pub(super) rank: Rank,
}
