use campfire_math::Num;
use campfire_sim::StableId;

use crate::scripts::error::CallError;
use crate::scripts::script_fn::ScriptFn;
use crate::units::action_id::ActionId;

/// A heal the pass applies to its target's life pool: from its source, none from a modifier the
/// mode applied, its amount before `calc_heal` and the heal scale, what gave it, the ability
/// whose cast, delivery or modifier gave it, and the depth of the chain of events that gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Heal {
    pub(crate) source: Option<StableId>,
    pub(crate) target: StableId,
    pub(crate) amount: Num,
    pub(crate) cause: HealCause,
    pub(crate) ability: Option<ActionId>,
    pub(crate) depth: u8,
}

/// What turns each heal of the pass into its amount before the heal scale: the mode's
/// `calc_heal`, which the mode gives combat when its script defines one.
pub(crate) type HealWeigher = ScriptFn<Heal, Result<Num, CallError>>;

/// What gave a heal: an ability's or a modifier's effect, or the source's leech, by the life a
/// damage of it took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealCause {
    Effect,
    Leech,
}
