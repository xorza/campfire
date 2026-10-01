use bevy_ecs::resource::Resource;

use crate::combat::combat_rules::CombatRules;
use crate::stats::stat_book::StatBook;

/// The places among the stats of those the mode's `[combat]` binds: leech from attacks and from
/// other damage, and the heal scale. Package data, not state; without a binding, its rule does
/// nothing.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BoundStats {
    pub(crate) leech_attack: Option<u16>,
    pub(crate) leech_other: Option<u16>,
    pub(crate) heal_scale: Option<u16>,
}

impl BoundStats {
    /// The bindings of `rules` among the stats of `book`, which the load checked declares each.
    pub(crate) fn new(rules: &CombatRules, book: &StatBook) -> BoundStats {
        let leech = rules.leech.as_ref();
        let index = |stat| book.index(stat).expect("the load checked the bound stats");
        BoundStats {
            leech_attack: leech.and_then(|leech| leech.attack.as_ref()).map(index),
            leech_other: leech.and_then(|leech| leech.other.as_ref()).map(index),
            heal_scale: rules.heal_scale.as_ref().map(index),
        }
    }
}
