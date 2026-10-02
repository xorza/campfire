use std::collections::BTreeMap;
use std::num::NonZeroU32;

use campfire_content::PackagePath;
use serde::Deserialize;

use crate::scripts::state_decl::StateDecl;
use crate::stats::stat::Stat;
use crate::stats::stat_change::StatChange;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::number::Number;
use crate::values::param::Param;

/// A modifier as its data file declares it, in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModifierData {
    pub script: Option<PackagePath>,
    /// Absent: until removed.
    pub duration_ms: Option<Number>,
    pub interval_ms: Option<Number>,
    pub stacks_expire_ms: Option<Number>,
    #[serde(default)]
    pub reapply: Reapply,
    /// Absent: no limit.
    pub max_stacks: Option<NonZeroU32>,
    /// Per stack.
    #[serde(default)]
    pub stats: BTreeMap<Stat, StatChange>,
    /// The tags it grants its carrier.
    #[serde(default)]
    pub tags: Vec<DeclaredName>,
    /// The modifier ends when the shield is spent.
    pub shield: Option<Number>,
    pub aura: Option<AuraData>,
    /// The units of its player a player modifier holds on; absent, all of them.
    pub affects: Option<FilterData>,
    #[serde(default)]
    pub params: BTreeMap<String, Param>,
    #[serde(default)]
    pub state: BTreeMap<String, StateDecl>,
}

impl ModifierData {
    /// Every number field that reads a param, `{ param = "<name>" }`: the names it reads.
    pub fn param_refs(&self) -> impl Iterator<Item = &str> + '_ {
        [
            self.duration_ms.as_ref(),
            self.interval_ms.as_ref(),
            self.stacks_expire_ms.as_ref(),
            self.shield.as_ref(),
            self.aura.as_ref().map(|aura| &aura.radius),
        ]
        .into_iter()
        .flatten()
        .chain(self.stats.values().map(|change| &change.value))
        .filter_map(Number::param)
    }
}

/// What a second application of a modifier from the same source does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reapply {
    #[default]
    Refresh,
    Stack,
    Ignore,
}

impl Reapply {
    /// The stacks of an instance of `stacks` after one more application from its source, up to
    /// `max_stacks`.
    pub(crate) const fn stacks(self, stacks: u32, max_stacks: Option<NonZeroU32>) -> u32 {
        match (self, max_stacks) {
            (Reapply::Stack, Some(max)) if stacks >= max.get() => stacks,
            (Reapply::Stack, _) => stacks.saturating_add(1),
            (Reapply::Refresh | Reapply::Ignore, _) => stacks,
        }
    }
}

/// A modifier held on the units within `radius` that `affects` selects.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuraData {
    pub radius: Number,
    pub affects: FilterData,
    pub modifier: String,
}
