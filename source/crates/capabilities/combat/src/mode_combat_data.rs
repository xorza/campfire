use campfire_declare::{Section, SectionTable};
use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_script::{DataSection, DataTable, NameKind, UnknownName};
use campfire_stats::PoolId;
use campfire_values::{Names, Stat};
use serde::Deserialize;

use crate::armor::ArmorShare;

/// The mode's `[combat]` section: its damage kinds, its assist window, the pool that is life,
/// the stats its damage rules read, and its armors.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeCombatData {
    /// The kinds of damage its scripts deal and its `calc_damage` weighs.
    #[serde(default)]
    pub damage_kinds: Vec<DeclaredName>,
    /// How long after damage a unit counts as having assisted a kill.
    pub assist_window_ms: Option<u64>,
    /// The pool that is life, which damage takes from and heals add to.
    pub life: Option<DeclaredName>,
    pub leech: Option<LeechData>,
    /// The stat that scales the heals its unit receives, by one plus its value.
    pub heal_scale: Option<Stat>,
    /// Its armors by name, each a share of damage by damage kind, which a unit type names.
    #[serde(default)]
    pub armors: BTreeMap<DeclaredName, BTreeMap<DeclaredName, ArmorShare>>,
}

/// The stats by which a source heals from the life its damage took: `attack` from an attack's,
/// `other` from any other damage's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeechData {
    pub attack: Option<Stat>,
    pub other: Option<Stat>,
}

impl ModeCombatData {
    /// The id among `pools` of the life pool it names; `None` when it names none, and an error
    /// for a pool `pools` does not hold.
    pub fn life_pool(&self, pools: &Names<PoolId>) -> Result<Option<PoolId>, UnknownName> {
        let Some(life) = &self.life else {
            return Ok(None);
        };
        let pool = pools.named(life.as_str());
        Ok(Some(
            pool.ok_or_else(|| UnknownName::new(NameKind::Pool, life))?,
        ))
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

impl DataSection for ModeCombatData {
    const TABLE: DataTable = DataTable::ModeCombat;
}

impl DataSection for LeechData {
    const TABLE: DataTable = DataTable::Leech;
}

impl Section for ModeCombatData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "combat";
}
