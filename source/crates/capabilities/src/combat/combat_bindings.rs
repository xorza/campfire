use bevy_ecs::resource::Resource;

use crate::combat::combat_rules::CombatRules;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_id::StatId;

/// What the mode's `[combat]` binds besides its life pool: the places among the stats of leech
/// from attacks and from other damage, and of the heal scale. Package data, not state; without a
/// stat binding, its rule does nothing. A match with combat has it, as the load checked the
/// mode names its life pool.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CombatBindings {
    pub(crate) leech_attack: Option<StatId>,
    pub(crate) leech_other: Option<StatId>,
    pub(crate) heal_scale: Option<StatId>,
}

impl CombatBindings {
    /// The bindings of `rules` among the stats of `book`, which the load checked declares each.
    pub(crate) fn new(rules: &CombatRules, book: &StatBook) -> CombatBindings {
        let leech = rules.leech.as_ref();
        let index = |stat| book.named(stat).expect("the load checked the bound stats");
        CombatBindings {
            leech_attack: leech.and_then(|leech| leech.attack.as_ref()).map(index),
            leech_other: leech.and_then(|leech| leech.other.as_ref()).map(index),
            heal_scale: rules.heal_scale.as_ref().map(index),
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::combat::combat_bindings::CombatBindings;

    impl CombatBindings {
        /// The bindings of no stat.
        pub(crate) const UNBOUND: CombatBindings = CombatBindings {
            leech_attack: None,
            leech_other: None,
            heal_scale: None,
        };
    }
}
