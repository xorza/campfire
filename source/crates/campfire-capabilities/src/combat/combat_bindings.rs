use bevy_ecs::resource::Resource;

use crate::combat::mode_combat_data::ModeCombatData;
use crate::stats::stat_id::StatId;
use crate::values::stat::Stat;

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
    /// The bindings of `rules`, each stat resolved by `stat`; the first error `stat` gives.
    pub(crate) fn new<E>(
        rules: &ModeCombatData,
        stat: impl Fn(&Stat) -> Result<StatId, E>,
    ) -> Result<CombatBindings, E> {
        let leech = rules.leech.as_ref();
        let bound = |held: Option<&Stat>| held.map(&stat).transpose();
        Ok(CombatBindings {
            leech_attack: bound(leech.and_then(|leech| leech.attack.as_ref()))?,
            leech_other: bound(leech.and_then(|leech| leech.other.as_ref()))?,
            heal_scale: bound(rules.heal_scale.as_ref())?,
        })
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
