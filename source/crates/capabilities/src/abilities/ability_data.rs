use std::collections::BTreeMap;
use std::str::FromStr;

use campfire_content::PackagePath;
use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::units::filter_data::FilterData;
use crate::units::number::Number;
use crate::units::param::Param;
use crate::units::ranked::Ranked;
use crate::units::relation::Relation;
use crate::units::state_decl::StateDecl;

/// An ability as its data file declares it, in milliseconds. Each capability field may hold one
/// value or one per rank. The release loads every field, and runs only the targeting, range,
/// cooldown, cost, cast time and params yet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityData {
    /// The script, if the ability needs one.
    pub script: Option<PackagePath>,
    pub targeting: Targeting,
    pub range: Option<Ranked<Range>>,
    pub cooldown_ms: Option<Ranked<u64>>,
    /// In the caster's resource.
    pub cost: Option<Ranked<u64>>,
    pub cast_time_ms: Option<Ranked<u64>>,
    /// A target beyond range is moved in, instead of the caster walking.
    #[serde(default)]
    pub clamp_to_range: bool,
    pub toggle: Option<Toggle>,
    pub channel: Option<ChannelData>,
    /// The modifier held while the toggle is on or the channel runs.
    pub hold: Option<String>,
    pub charges: Option<ChargesData>,
    /// A charged cast.
    pub charge: Option<ChargeData>,
    /// The modifier held while the ability has a rank.
    pub passive_modifier: Option<String>,
    /// The passive modifier is held only while the ability is off cooldown.
    #[serde(default)]
    pub passive_while_ready: bool,
    pub projectile: Option<ProjectileData>,
    pub area: Option<AreaData>,
    /// Values for the script, as `ctx.p` reads them.
    #[serde(default)]
    pub params: BTreeMap<String, Param>,
    /// The state of each projectile the ability fires.
    #[serde(default)]
    pub projectile_state: BTreeMap<String, StateDecl>,
}

/// A toggle's cost, in the caster's resource.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Toggle {
    CostPerAttack(Ranked<Number>),
    CostPerSecond(Ranked<Number>),
}

/// A channel, which starts after `on_cast` and calls `on_channel_tick` every `tick_ms`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelData {
    pub duration_ms: Ranked<Number>,
    pub tick_ms: Ranked<Number>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChargesData {
    pub max: Ranked<Number>,
    pub recharge_ms: Ranked<Number>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChargeData {
    pub max_ms: Ranked<Number>,
}

/// The projectile `ctx.projectile` fires. Its speed and width are in meters a second and meters.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectileData {
    pub speed: Ranked<Number>,
    pub width: Option<Ranked<Number>>,
    /// Default: the ability's.
    pub range: Option<Ranked<Range>>,
    pub stop_on_hit: Option<bool>,
    pub once_per_cast: Option<bool>,
    /// A filter of the units it hits.
    pub hits: Option<FilterData>,
    pub sight_radius: Option<Ranked<Number>>,
    pub collide: Option<bool>,
}

/// The area `ctx.area` places.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaData {
    pub radius: Ranked<Number>,
    pub delay_ms: Option<Ranked<Number>>,
    pub duration_ms: Option<Ranked<Number>>,
    /// A filter of the units `on_area_trigger` receives.
    pub affects: Option<FilterData>,
    pub inside: Option<AreaInside>,
}

/// The modifiers an area holds on the units inside it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaInside {
    #[serde(rename = "self")]
    pub caster: Option<String>,
    pub allies: Option<String>,
    pub enemies: Option<String>,
}

impl AbilityData {
    /// The length of every per-rank array it holds: its capability fields' and its params'.
    pub fn rank_counts(&self) -> impl Iterator<Item = usize> + '_ {
        let numbers = |ranked: Option<&Ranked<Number>>| ranked.and_then(Ranked::ranks);
        let toggle = self.toggle.as_ref().map(|toggle| match toggle {
            Toggle::CostPerAttack(cost) | Toggle::CostPerSecond(cost) => cost,
        });
        let projectile = self.projectile.as_ref();
        let area = self.area.as_ref();
        [
            self.range.as_ref().and_then(Ranked::ranks),
            self.cooldown_ms.as_ref().and_then(Ranked::ranks),
            self.cost.as_ref().and_then(Ranked::ranks),
            self.cast_time_ms.as_ref().and_then(Ranked::ranks),
            numbers(toggle),
            numbers(self.channel.as_ref().map(|channel| &channel.duration_ms)),
            numbers(self.channel.as_ref().map(|channel| &channel.tick_ms)),
            numbers(self.charges.as_ref().map(|charges| &charges.max)),
            numbers(self.charges.as_ref().map(|charges| &charges.recharge_ms)),
            numbers(self.charge.as_ref().map(|charge| &charge.max_ms)),
            numbers(projectile.map(|projectile| &projectile.speed)),
            numbers(projectile.and_then(|projectile| projectile.width.as_ref())),
            projectile
                .and_then(|projectile| projectile.range.as_ref())
                .and_then(Ranked::ranks),
            numbers(projectile.and_then(|projectile| projectile.sight_radius.as_ref())),
            numbers(area.map(|area| &area.radius)),
            numbers(area.and_then(|area| area.delay_ms.as_ref())),
            numbers(area.and_then(|area| area.duration_ms.as_ref())),
        ]
        .into_iter()
        .chain(self.params.values().map(Param::ranks))
        .flatten()
    }

    /// Every number field that reads a param, `{ param = "<name>" }`: the names it reads.
    pub fn param_refs(&self) -> impl Iterator<Item = &str> + '_ {
        let toggle = self.toggle.as_ref().map(|toggle| match toggle {
            Toggle::CostPerAttack(cost) | Toggle::CostPerSecond(cost) => cost,
        });
        let projectile = self.projectile.as_ref();
        let area = self.area.as_ref();
        [
            toggle,
            self.channel.as_ref().map(|channel| &channel.duration_ms),
            self.channel.as_ref().map(|channel| &channel.tick_ms),
            self.charges.as_ref().map(|charges| &charges.max),
            self.charges.as_ref().map(|charges| &charges.recharge_ms),
            self.charge.as_ref().map(|charge| &charge.max_ms),
            projectile.map(|projectile| &projectile.speed),
            projectile.and_then(|projectile| projectile.width.as_ref()),
            projectile.and_then(|projectile| projectile.sight_radius.as_ref()),
            area.map(|area| &area.radius),
            area.and_then(|area| area.delay_ms.as_ref()),
            area.and_then(|area| area.duration_ms.as_ref()),
        ]
        .into_iter()
        .flatten()
        .flat_map(Ranked::values)
        .filter_map(Number::param)
    }

    /// The ids of the modifiers its data names: the one it holds, its passive and those its area
    /// holds.
    pub fn modifiers(&self) -> impl Iterator<Item = &str> + '_ {
        let inside = self.area.as_ref().and_then(|area| area.inside.as_ref());
        [
            self.hold.as_ref(),
            self.passive_modifier.as_ref(),
            inside.and_then(|inside| inside.caster.as_ref()),
            inside.and_then(|inside| inside.allies.as_ref()),
            inside.and_then(|inside| inside.enemies.as_ref()),
        ]
        .into_iter()
        .flatten()
        .map(String::as_str)
    }

    /// The filters its data names: its projectile's hits and its area's affects.
    pub fn filters(&self) -> impl Iterator<Item = &FilterData> + '_ {
        [
            self.projectile
                .as_ref()
                .and_then(|projectile| projectile.hits.as_ref()),
            self.area.as_ref().and_then(|area| area.affects.as_ref()),
        ]
        .into_iter()
        .flatten()
    }
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

/// How far an ability reaches. In data: meters as a decimal string, or `global`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Meters(Num),
    Global,
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
