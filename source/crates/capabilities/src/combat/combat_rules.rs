use std::collections::BTreeMap;

use serde::Deserialize;

use crate::combat::damage_kind::DamageKind;
use crate::stats::pool_data::PoolData;
use crate::stats::pool_id::PoolId;
use crate::values::declared_name::DeclaredName;
use crate::values::stat::Stat;

/// The mode's `[combat]` section: its damage kinds, its assist window, the pool that is life,
/// and the stats its damage rules read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRules {
    /// The kinds of damage its scripts deal and its `calc_damage` weighs.
    #[serde(default)]
    pub damage_kinds: Vec<DeclaredName>,
    /// How long after damage a unit counts as having assisted a kill.
    pub assist_window_ms: Option<u64>,
    /// The pool that is life, which damage takes from and heals add to.
    pub life: Option<DeclaredName>,
    pub leech: Option<Leech>,
    /// The stat that scales the heals its unit receives, by one plus its value.
    pub heal_scale: Option<Stat>,
}

/// The stats by which a source heals from the life its damage took: `attack` from an attack's,
/// `other` from any other damage's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Leech {
    pub attack: Option<Stat>,
    pub other: Option<Stat>,
}

impl CombatRules {
    /// The most damage kinds it declares.
    pub const DAMAGE_KIND_LIMIT: usize = DamageKind::LIMIT;

    /// The id among `pools` of the life pool it names, which the load checked `pools` declares;
    /// `None` when it names none.
    pub fn life_pool(&self, pools: &BTreeMap<DeclaredName, PoolData>) -> Option<PoolId> {
        let life = self.life.as_ref()?;
        Some(PoolId::named(pools, life).expect("the load checked the life pool"))
    }

    /// Every stat its rules read.
    pub fn stats(&self) -> impl Iterator<Item = &Stat> {
        let leech = self.leech.as_ref();
        [
            leech.and_then(|leech| leech.attack.as_ref()),
            leech.and_then(|leech| leech.other.as_ref()),
            self.heal_scale.as_ref(),
        ]
        .into_iter()
        .flatten()
    }
}
