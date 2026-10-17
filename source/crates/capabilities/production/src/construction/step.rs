use bevy_ecs::entity::Entity;
use campfire_actions::ChargeRule;
use campfire_common::Ticks;
use campfire_math::{Num, Vec3};
use campfire_units::PoolCost;
use campfire_values::{Rank, UnitType};

/// What a builder does with its build order this tick.
#[derive(Debug, Clone, Copy)]
pub enum Step {
    /// It walks to the point of its target's box nearest it.
    Walk(Vec3),
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
pub struct Start {
    pub(super) unit_type: UnitType,
    pub pos: Vec3,
    pub(super) angle: Num,
    pub(super) cost: PoolCost,
    pub(super) cooldown: Ticks,
    pub(super) charges: Option<ChargeRule>,
    pub rank: Rank,
}
