use std::collections::BTreeMap;

use campfire_math::Num;
use serde::Deserialize;

use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;
use crate::values::stat::Stat;

/// A param, as `ctx.p` reads it: one value or one per rank, or a scaling table, `base +
/// per_level × (level − 1) + Σ ratio × stat + Σ bonus ratio × (stat − type's value)` of the
/// source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Param {
    Ranked(Ranked<Scalar>),
    Scaling(Scaling),
}

/// A scaling table: the base, per rank or one, its gain a level, the ratio of each stat it
/// scales with, by the stat's name, and under `bonus` the ratio of each stat's part above its
/// type's value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Scaling {
    pub base: Ranked<Scalar>,
    #[serde(default, deserialize_with = "Scalar::num")]
    pub per_level: Num,
    #[serde(default, deserialize_with = "Scalar::nums")]
    pub bonus: BTreeMap<Stat, Num>,
    #[serde(flatten, deserialize_with = "Scalar::nums")]
    pub ratios: BTreeMap<Stat, Num>,
}

impl Param {
    /// Every stat its scaling table names, its bonus ratios' among them.
    pub fn stats(&self) -> impl Iterator<Item = &Stat> {
        let scaling = match self {
            Param::Ranked(_) => None,
            Param::Scaling(scaling) => Some(scaling),
        };
        scaling
            .into_iter()
            .flat_map(|scaling| scaling.ratios.keys().chain(scaling.bonus.keys()))
    }

    /// How many ranks it has values for; `None` when it fits any rank.
    pub fn ranks(&self) -> Option<usize> {
        match self {
            Param::Ranked(ranked) => ranked.ranks(),
            Param::Scaling(scaling) => scaling.base.ranks(),
        }
    }
}
