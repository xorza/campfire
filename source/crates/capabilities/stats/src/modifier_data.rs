use std::collections::BTreeMap;
use std::num::NonZeroU32;

use campfire_common::Ticks;
use campfire_math::Num;
use campfire_script::{DataSection, DataTable, StateDecl};
use campfire_sim::TickRate;
use campfire_values::{DeclaredName, FilterData, Number, PackagePath, Param, Stat};
use serde::Deserialize;

use crate::stat_change::StatChange;

/// A modifier as its data file declares it, in milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
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
    pub params: BTreeMap<DeclaredName, Param>,
    #[serde(default)]
    pub state: BTreeMap<DeclaredName, StateDecl>,
}

impl ModifierData {
    /// `ms` milliseconds of one of its times, up to the next whole one, in ticks at `rate`;
    /// `None` for a negative time or one too large to count.
    pub fn ticks(ms: Num, rate: TickRate) -> Option<Ticks> {
        let ms = u64::try_from(ms.ceil()).ok()?;
        rate.duration(ms)
    }

    /// Its times, as its data gives them: its duration, its interval and its stacks' expiry.
    pub fn times(&self) -> impl Iterator<Item = &Number> {
        [
            self.duration_ms.as_ref(),
            self.interval_ms.as_ref(),
            self.stacks_expire_ms.as_ref(),
        ]
        .into_iter()
        .flatten()
    }

    /// Every number field that reads a param, `{ param = "<name>" }`: the names it reads.
    pub fn param_refs(&self) -> impl Iterator<Item = &DeclaredName> + '_ {
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
            (Self::Stack, Some(max)) if stacks >= max.get() => stacks,
            (Self::Stack, _) => stacks.saturating_add(1),
            (Self::Refresh | Self::Ignore, _) => stacks,
        }
    }
}

/// A modifier held on the units within `radius` that `affects` selects.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuraData {
    pub radius: Number,
    pub affects: FilterData,
    pub modifier: DeclaredName,
}

impl DataSection for ModifierData {
    const TABLE: DataTable = DataTable::Modifier;
}

impl DataSection for AuraData {
    const TABLE: DataTable = DataTable::Aura;
}
