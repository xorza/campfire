use std::collections::BTreeMap;

use campfire_math::Num;
use serde::Deserialize;

use crate::ranked::Ranked;
use crate::scalar::Scalar;
use crate::stat::Stat;

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
    pub base: Ranked<Num>,
    #[serde(default)]
    pub per_level: Num,
    #[serde(default)]
    pub bonus: BTreeMap<Stat, Num>,
    #[serde(flatten)]
    pub ratios: BTreeMap<Stat, Num>,
}

impl Param {
    /// The values it can have, each rank's, as numbers, `None` for one past what a number holds;
    /// a scaling param's base's, as its source's stats add to it only as it applies.
    pub fn numbers(&self) -> impl Iterator<Item = Option<Num>> + '_ {
        let (ranked, base): (&[Scalar], &[Num]) = match self {
            Param::Ranked(ranked) => (ranked.values(), &[]),
            Param::Scaling(scaling) => (&[], scaling.base.values()),
        };
        let ranked = ranked.iter().map(|value| value.to_num());
        ranked.chain(base.iter().copied().map(Some))
    }

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
    pub const fn ranks(&self) -> Option<usize> {
        match self {
            Param::Ranked(ranked) => ranked.ranks(),
            Param::Scaling(scaling) => scaling.base.ranks(),
        }
    }
}
