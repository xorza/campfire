use campfire_math::Num;
use serde::Deserialize;

use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;

/// A param, as `ctx.p` reads it: one value or one per rank, or a scaling table, `base +
/// per_level × level + Σ ratio × stat` of the source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Param {
    Ranked(Ranked<Scalar>),
    Scaling(Scaling),
}

/// A scaling table: the base, per rank or one, and the ratio of each stat it scales with.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scaling {
    pub base: Ranked<Scalar>,
    pub per_level: Option<Scalar>,
    pub ad: Option<Scalar>,
    pub bonus_ad: Option<Scalar>,
    pub ap: Option<Scalar>,
    pub max_health: Option<Scalar>,
    pub bonus_health: Option<Scalar>,
    pub armor: Option<Scalar>,
    pub magic_resist: Option<Scalar>,
}

impl Param {
    /// Its value at `rank`; `None` past its ranks or when it overflows. Until levels and stats
    /// exist, every unit is level 1 and every stat a scaling table names is 0.
    pub fn at(&self, rank: u8) -> Option<Scalar> {
        match self {
            Param::Ranked(ranked) => ranked.at(rank),
            Param::Scaling(scaling) => {
                let base = scaling.base.at(rank)?.to_num()?;
                let per_level = scaling.per_level.map_or(Some(Num::ZERO), Scalar::to_num)?;
                Some(Scalar::Decimal(base.checked_add(per_level)?))
            }
        }
    }

    /// How many ranks it has values for; `None` when it fits any rank.
    pub fn ranks(&self) -> Option<usize> {
        match self {
            Param::Ranked(ranked) => ranked.ranks(),
            Param::Scaling(scaling) => scaling.base.ranks(),
        }
    }
}
