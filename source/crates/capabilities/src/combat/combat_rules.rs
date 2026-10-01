use serde::Deserialize;

use crate::stats::stat::Stat;
use crate::values::declared_name::DeclaredName;

/// The mode's `[combat]` section: its damage kinds, its assist window, and the stats its damage
/// rules read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRules {
    /// The kinds of damage its scripts deal and its `calc_damage` weighs.
    #[serde(default)]
    pub damage_kinds: Vec<DeclaredName>,
    /// How long after damage a unit counts as having assisted a kill.
    pub assist_window_ms: Option<u64>,
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
