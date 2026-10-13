use std::num::NonZeroU8;

use crate::actions::action_range::ActionRange;
use crate::players::resource_amount::ResourceAmount;
use crate::stats::pool_cost::PoolCost;

/// An action's capability fields at one rank, as data gives them: times in milliseconds, its
/// cost in its caster's pools, and in its caster's player's resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankFields {
    pub range: ActionRange,
    pub cooldown_ms: u64,
    pub cost: PoolCost,
    pub resource_cost: Vec<ResourceAmount>,
    pub windup_ms: u64,
    pub charges: Option<RankCharges>,
    pub toggle: Option<RankToggle>,
    pub channel: Option<RankChannel>,
    /// A charged action's most, in milliseconds.
    pub charge_ms: Option<u64>,
}

/// A channel at one rank: how long it runs, and the time between its ticks, both positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankChannel {
    pub duration_ms: u64,
    pub tick_ms: u64,
}

/// A toggle's cost at one rank: in the caster's pools, paid as each attack goes off, or at each
/// whole second it is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankToggle {
    pub per: TogglePer,
    pub cost: PoolCost,
}

/// When a toggle pays its cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TogglePer {
    Attack,
    Second,
}

/// An action's charges at one rank: how many it holds at most, and how long one takes to come
/// back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankCharges {
    pub max: NonZeroU8,
    pub recharge_ms: u64,
}
