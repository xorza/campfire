use std::collections::BTreeMap;

use campfire_math::Num;
use serde::Deserialize;

use crate::stats::stat::Stat;
use crate::values::scalar::Scalar;

/// A unit type's `stats` section: each stat's value at level 1, and what it gains a level.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct StatsData(pub BTreeMap<Stat, StatValue>);

/// `{ base, per_level }`: the value at level `n` is `base + per_level × (n − 1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatValue {
    pub base: Scalar,
    pub per_level: Option<Scalar>,
}

impl StatsData {
    /// Whether the type declares `stat`.
    pub fn declares(&self, stat: &Stat) -> bool {
        self.0.contains_key(stat)
    }

    /// `stat` at `level`, from 1; `None` when the type does not declare it, or it overflows.
    pub fn at(&self, stat: &Stat, level: u32) -> Option<Num> {
        let value = self.0.get(stat)?;
        let base = value.base.to_num()?;
        let per_level = value.per_level.map_or(Some(Num::ZERO), Scalar::to_num)?;
        base.checked_add(per_level.checked_mul_int(i64::from(level.checked_sub(1)?))?)
    }
}

#[cfg(test)]
mod tests;
