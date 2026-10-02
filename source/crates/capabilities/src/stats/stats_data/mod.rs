use std::collections::BTreeMap;

use campfire_math::Num;
use serde::Deserialize;

use crate::stats::stat::Stat;
use crate::values::scalar::Scalar;

/// A unit type's `stats` section: each stat's value at level 1, and what it gains a level.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct StatsData(pub BTreeMap<Stat, StatValue>);

/// `{ base, per_level }`: the value at level `n` is `base + per_level × (n − 1)`, `per_level`
/// 0 when it gives none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatValue {
    #[serde(deserialize_with = "Scalar::num")]
    pub base: Num,
    #[serde(default, deserialize_with = "Scalar::num")]
    pub per_level: Num,
}

impl StatsData {
    /// Whether the type declares `stat`.
    pub fn declares(&self, stat: &Stat) -> bool {
        self.0.contains_key(stat)
    }

    /// `stat` at `level`, from 1; `None` when the type does not declare it, or it overflows.
    pub fn at(&self, stat: &Stat, level: u32) -> Option<Num> {
        let value = self.0.get(stat)?;
        let levels = i64::from(level.checked_sub(1)?);
        value
            .base
            .checked_add(value.per_level.checked_mul_int(levels)?)
    }
}

#[cfg(test)]
mod tests;
