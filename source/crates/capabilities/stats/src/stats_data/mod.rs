use std::collections::BTreeMap;

use campfire_math::Num;
use campfire_values::Stat;
use serde::Deserialize;

/// A unit type's `stats` section: each stat's value at level 1, and what it gains a level.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct StatsData(pub(crate) BTreeMap<Stat, StatValue>);

/// `{ base, per_level }`: the value at level `n` is `base + per_level × (n − 1)`, `per_level`
/// 0 when it gives none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatValue {
    pub base: Num,
    #[serde(default)]
    pub per_level: Num,
}

/// The section of `values`, each stat's last value standing, as `BTreeMap` collects them.
impl FromIterator<(Stat, StatValue)> for StatsData {
    fn from_iter<I: IntoIterator<Item = (Stat, StatValue)>>(values: I) -> Self {
        Self(values.into_iter().collect())
    }
}

impl<const N: usize> From<[(Stat, StatValue); N]> for StatsData {
    fn from(values: [(Stat, StatValue); N]) -> Self {
        values.into_iter().collect()
    }
}

impl StatsData {
    /// Whether the type declares `stat`.
    pub fn declares(&self, stat: &Stat) -> bool {
        self.0.contains_key(stat)
    }
}

#[cfg(test)]
mod tests;
