use campfire_math::Num;
use campfire_sim::StableId;
use campfire_values::ActionId;
use serde::{Deserialize, Serialize};

/// A heal the pass applies to its target's life pool: from its source, none from a modifier the
/// mode applied, its amount before `calc_heal` and the heal scale, what gave it, the ability
/// whose cast, delivery or modifier gave it, and the depth of the chain of events that gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heal {
    pub source: Option<StableId>,
    pub target: StableId,
    pub amount: Num,
    pub cause: HealCause,
    pub action: Option<ActionId>,
    pub depth: u8,
}

/// What gave a heal: an ability's or a modifier's effect; the source's leech, by the life a
/// damage of it took; or the upkeep of a capability above combat, as an airfield's heal of the
/// units that rest at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealCause {
    Effect,
    Leech,
    Upkeep,
}
