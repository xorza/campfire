use serde::Deserialize;

use crate::units::ranked::Ranked;
use crate::units::scalar::Scalar;

/// A param, as `ctx.p` reads it: one value, one per rank, or a scaling table, `base + per_level
/// × level + Σ ratio × stat` of the source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Param {
    Value(Scalar),
    PerRank(Vec<Scalar>),
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
    /// How many ranks it has values for; `None` when it fits any rank.
    pub fn ranks(&self) -> Option<usize> {
        match self {
            Param::Value(_) => None,
            Param::PerRank(values) => Some(values.len()),
            Param::Scaling(scaling) => scaling.base.ranks(),
        }
    }
}
