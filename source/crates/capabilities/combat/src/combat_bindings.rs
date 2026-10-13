use campfire_stats::StatId;
use campfire_values::Stat;

use crate::mode_combat_data::ModeCombatData;

/// What the mode's `[combat]` binds besides its life pool: the places among the stats of leech
/// from attacks and from other damage, and of the heal scale; without a stat binding, its rule
/// does nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CombatBindings {
    pub(crate) leech_attack: Option<StatId>,
    pub(crate) leech_other: Option<StatId>,
    pub(crate) heal_scale: Option<StatId>,
}

impl CombatBindings {
    /// The bindings of `rules`, each stat resolved by `stat`; the first error `stat` gives.
    pub fn new<E>(
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
