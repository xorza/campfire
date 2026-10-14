use campfire_math::Num;
use campfire_script::{CallError, ScriptFn};
use campfire_sim::StableId;
use campfire_values::ActionId;

/// A heal the pass applies to its target's life pool: from its source, none from a modifier the
/// mode applied, its amount before `calc_heal` and the heal scale, what gave it, the ability
/// whose cast, delivery or modifier gave it, and the depth of the chain of events that gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heal {
    pub source: Option<StableId>,
    pub target: StableId,
    pub amount: Num,
    pub cause: HealCause,
    pub action: Option<ActionId>,
    pub depth: u8,
}

/// What turns each heal of the pass into its amount before the heal scale: the mode's
/// `calc_heal`, which the mode gives combat when its script defines one.
pub type HealWeigher = ScriptFn<Heal, Result<Num, CallError>>;

/// What gave a heal: an ability's or a modifier's effect, or the source's leech, by the life a
/// damage of it took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealCause {
    Effect,
    Leech,
}
