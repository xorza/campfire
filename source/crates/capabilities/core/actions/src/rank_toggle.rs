use campfire_units::PoolCost;

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
