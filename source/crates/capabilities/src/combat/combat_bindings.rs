use std::collections::BTreeMap;

use bevy_ecs::resource::Resource;

use crate::combat::combat_rules::CombatRules;
use crate::stats::pool_data::PoolData;
use crate::stats::pool_id::PoolId;
use crate::stats::stat_book::StatBook;
use crate::values::declared_name::DeclaredName;

/// What the mode's `[combat]` binds: the life pool, and the places among the stats of leech from
/// attacks and from other damage, and of the heal scale. Package data, not state; without a
/// stat binding, its rule does nothing, and until a mode binds one, the life pool is the first.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CombatBindings {
    pub(crate) life: PoolId,
    pub(crate) leech_attack: Option<u16>,
    pub(crate) leech_other: Option<u16>,
    pub(crate) heal_scale: Option<u16>,
}

impl CombatBindings {
    /// The bindings of `rules` among `pools` and the stats of `book`, which the load checked
    /// declare each.
    pub(crate) fn new(
        rules: &CombatRules,
        pools: &BTreeMap<DeclaredName, PoolData>,
        book: &StatBook,
    ) -> CombatBindings {
        let leech = rules.leech.as_ref();
        let index = |stat| book.index(stat).expect("the load checked the bound stats");
        CombatBindings {
            life: rules.life_pool(pools).unwrap_or(PoolId::FIRST),
            leech_attack: leech.and_then(|leech| leech.attack.as_ref()).map(index),
            leech_other: leech.and_then(|leech| leech.other.as_ref()).map(index),
            heal_scale: rules.heal_scale.as_ref().map(index),
        }
    }
}

impl Default for CombatBindings {
    fn default() -> CombatBindings {
        CombatBindings {
            life: PoolId::FIRST,
            leech_attack: None,
            leech_other: None,
            heal_scale: None,
        }
    }
}
