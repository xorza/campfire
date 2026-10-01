use std::collections::BTreeMap;

use serde::Deserialize;

use crate::stats::stat::Stat;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;

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
    pub per_level: Option<Scalar>,
    #[serde(default)]
    pub bonus: BTreeMap<Stat, Scalar>,
    #[serde(flatten)]
    pub ratios: BTreeMap<Stat, Scalar>,
}

impl Param {
    /// Its value at `rank`; `None` past its ranks. Until a param reads its source, the source
    /// is level 1 and every stat a scaling table names is 0, so a table's value is its base.
    pub fn at(&self, rank: u8) -> Option<Scalar> {
        match self {
            Param::Ranked(ranked) => ranked.at(rank),
            Param::Scaling(scaling) => scaling.base.at(rank),
        }
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
    pub fn ranks(&self) -> Option<usize> {
        match self {
            Param::Ranked(ranked) => ranked.ranks(),
            Param::Scaling(scaling) => scaling.base.ranks(),
        }
    }
}
