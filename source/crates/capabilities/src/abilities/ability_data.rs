use std::collections::BTreeMap;
use std::str::FromStr;

use crate::combat::team::Team;

use campfire_content::PackagePath;
use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// The `[abilities.<id>]` tables of a data file, such as a hero's or the player spells'. Its
/// other tables belong to other capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AbilityTables {
    #[serde(default)]
    pub abilities: BTreeMap<String, AbilityData>,
}

/// An ability as its data file declares it, in milliseconds. Each capability field may hold one
/// value or one per rank. Fields of mechanisms the engine does not run yet, such as projectiles
/// or toggles, are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AbilityData {
    /// The script, if the ability needs one.
    pub script: Option<PackagePath>,
    pub targeting: Targeting,
    pub range: Option<Ranked<Range>>,
    pub cooldown_ms: Option<Ranked<u64>>,
    /// In the caster's resource.
    pub cost: Option<Ranked<u64>>,
    pub cast_time_ms: Option<Ranked<u64>>,
    /// Values for the script, as `ctx.p` reads them.
    #[serde(default)]
    pub params: BTreeMap<String, Param>,
}

/// What an ability targets. In data: `none`, `point`, `direction`, or a filter of the units it
/// may target, such as `enemies`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Targeting {
    None,
    Point,
    Direction,
    Unit(Relation),
}

/// Which units a filter selects, relative to a unit: its enemies, its allies, which include
/// itself, or all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Enemies,
    Allies,
    All,
}

/// How far an ability reaches. In data: meters as a decimal string, or `global`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Meters(Num),
    Global,
}

/// One value for every rank, or one per rank.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Ranked<T> {
    One(T),
    PerRank(Vec<T>),
}

/// A script value: a TOML integer, or a decimal string, which is a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    Int(i64),
    Decimal(#[serde(deserialize_with = "decimal")] Num),
}

/// A param: one value, one per rank, or a scaling table, `base + per_level × level + Σ ratio ×
/// stat` of the source.
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

impl<T: Copy> Ranked<T> {
    /// The value at `rank`, from 1; `None` past the last rank.
    pub fn at(&self, rank: u8) -> Option<T> {
        match self {
            Ranked::One(value) => Some(*value),
            Ranked::PerRank(values) => values.get(usize::from(rank.checked_sub(1)?)).copied(),
        }
    }

    /// How many ranks it has values for; `None` for one value, which fits any rank.
    pub fn ranks(&self) -> Option<usize> {
        match self {
            Ranked::One(_) => None,
            Ranked::PerRank(values) => Some(values.len()),
        }
    }
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

impl Scalar {
    pub fn to_num(self) -> Option<Num> {
        match self {
            Scalar::Int(value) => Num::from_int(value),
            Scalar::Decimal(value) => Some(value),
        }
    }
}

impl Relation {
    /// Whether a unit of team `theirs` stands in this relation to one of team `ours`.
    pub(crate) const fn holds(self, ours: Team, theirs: Team) -> bool {
        match self {
            Relation::Enemies => ours.is_enemy_of(theirs),
            Relation::Allies => !ours.is_enemy_of(theirs),
            Relation::All => true,
        }
    }

    /// A filter's relation: `enemies`, `allies` or `all`.
    pub fn parse(filter: &str) -> Option<Relation> {
        match filter {
            "enemies" => Some(Relation::Enemies),
            "allies" => Some(Relation::Allies),
            "all" => Some(Relation::All),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for Targeting {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Targeting, D::Error> {
        let text = String::deserialize(deserializer)?;
        match text.as_str() {
            "none" => Ok(Targeting::None),
            "point" => Ok(Targeting::Point),
            "direction" => Ok(Targeting::Direction),
            filter if filter.contains(':') => Err(Error::custom(format!(
                "targeting {filter:?}: unit tags are not supported yet"
            ))),
            filter => Relation::parse(filter)
                .map(Targeting::Unit)
                .ok_or_else(|| Error::custom(format!("unknown targeting {filter:?}"))),
        }
    }
}

impl<'de> Deserialize<'de> for Range {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Range, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == "global" {
            return Ok(Range::Global);
        }
        Num::from_str(&text)
            .ok()
            .filter(|meters| *meters >= Num::ZERO)
            .map(Range::Meters)
            .ok_or_else(|| Error::custom(format!("range {text:?} is not meters or global")))
    }
}

/// A `Num` from a decimal string, such as `"3.5"`: data never holds a float.
fn decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Num, D::Error> {
    let text = String::deserialize(deserializer)?;
    Num::from_str(&text).map_err(Error::custom)
}
