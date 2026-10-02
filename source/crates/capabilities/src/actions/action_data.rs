use std::collections::BTreeMap;
use std::str::FromStr;

use campfire_content::PackagePath;
use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::actions::action_kind::ActionKind;
use crate::actions::delivery_data::DeliveryData;
use crate::actions::effect_data::EffectData;
use crate::actions::error::ActionField;
use crate::mode::resource_id::{ResourceAmount, ResourceId};
use crate::scripts::hook::Hook;
use crate::scripts::state_decl::StateDecl;
use crate::stats::pool_cost::PoolCost;
use crate::stats::pool_id::PoolId;
use crate::stats::stat::Stat;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::number::{Number, ParamRef};
use crate::values::param::Param;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;

/// An action as its package's `[actions.<id>]` declares it, in milliseconds. Each capability
/// field may hold one value or one per rank. The release loads every field, and runs those the
/// script API reference lists as running.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionData {
    #[serde(default)]
    pub kind: ActionKind,
    /// The script, if the action needs one.
    pub script: Option<PackagePath>,
    pub targeting: Targeting,
    pub range: Option<Ranked<RangeField>>,
    pub cooldown_ms: Option<Ranked<Number>>,
    /// In each pool of the caster it names, and each resource of the caster's player.
    #[serde(default)]
    pub cost: BTreeMap<DeclaredName, Ranked<Number>>,
    pub windup_ms: Option<Ranked<Number>>,
    /// A target beyond range is moved in, instead of the caster walking.
    #[serde(default)]
    pub clamp_to_range: bool,
    pub toggle: Option<Toggle>,
    pub channel: Option<ChannelData>,
    /// The modifier held while the toggle is on or the channel runs.
    pub hold: Option<DeclaredName>,
    pub charges: Option<ChargesData>,
    /// A charged cast.
    pub charge: Option<ChargeData>,
    /// The modifier held while the action has a rank.
    pub passive_modifier: Option<DeclaredName>,
    /// The passive modifier is held only while the action is off cooldown.
    #[serde(default)]
    pub passive_while_ready: bool,
    /// How it reaches what it affects, other than at once.
    pub delivery: Option<DeliveryData>,
    /// A weapon's stat of attacks a second, an `attack`'s alone.
    pub rate: Option<Stat>,
    /// A weapon's stat of its damage, an `attack`'s alone.
    pub damage: Option<Stat>,
    /// The kind of damage a weapon deals, an `attack`'s alone.
    pub damage_kind: Option<DeclaredName>,
    /// The mode's unit type a `train` makes, a train's alone.
    pub unit_type: Option<DeclaredName>,
    /// Values for the script, as `ctx.p` reads them.
    #[serde(default)]
    pub params: BTreeMap<DeclaredName, Param>,
    /// The state of each projectile the action fires.
    #[serde(default)]
    pub projectile_state: BTreeMap<DeclaredName, StateDecl>,
    /// The effects of its resolve, which queue before its script's `on_resolve`.
    #[serde(default)]
    pub on_resolve: Vec<EffectData>,
    /// The effects of each hit of its delivery, which queue before `on_hit`.
    #[serde(default)]
    pub on_hit: Vec<EffectData>,
    /// The effects of its delivery's end, which queue before `on_end`.
    #[serde(default)]
    pub on_end: Vec<EffectData>,
}

/// A toggle's cost, in each pool of the caster it names.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Toggle {
    CostPerAttack(BTreeMap<DeclaredName, Ranked<Number>>),
    CostPerSecond(BTreeMap<DeclaredName, Ranked<Number>>),
}

/// A channel, which starts after `on_resolve` and calls `on_channel_tick` every `tick_ms`.
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

impl ActionData {
    /// The length of every per-rank array it holds: its capability fields' and its params'.
    pub fn rank_counts(&self) -> impl Iterator<Item = usize> + '_ {
        let numbers = |ranked: Option<&Ranked<Number>>| ranked.and_then(Ranked::ranks);
        [
            self.range.as_ref().and_then(Ranked::ranks),
            self.cooldown_ms.as_ref().and_then(Ranked::ranks),
            self.windup_ms.as_ref().and_then(Ranked::ranks),
            numbers(self.channel.as_ref().map(|channel| &channel.duration_ms)),
            numbers(self.channel.as_ref().map(|channel| &channel.tick_ms)),
            numbers(self.charges.as_ref().map(|charges| &charges.max)),
            numbers(self.charges.as_ref().map(|charges| &charges.recharge_ms)),
            numbers(self.charge.as_ref().map(|charge| &charge.max_ms)),
        ]
        .into_iter()
        .chain(self.costs().map(Ranked::ranks))
        .chain(self.params.values().map(Param::ranks))
        .flatten()
    }

    /// Every pool or player resource it costs something in, its toggle's among them.
    pub fn cost_names(&self) -> impl Iterator<Item = &DeclaredName> + '_ {
        self.cost
            .keys()
            .chain(self.toggle_cost().into_iter().flat_map(BTreeMap::keys))
    }

    /// Each amount of its cost and its toggle's, in its pools.
    fn costs(&self) -> impl Iterator<Item = &Ranked<Number>> + '_ {
        self.cost
            .values()
            .chain(self.toggle_cost().into_iter().flat_map(BTreeMap::values))
    }

    fn toggle_cost(&self) -> Option<&BTreeMap<DeclaredName, Ranked<Number>>> {
        self.toggle.as_ref().map(|toggle| match toggle {
            Toggle::CostPerAttack(cost) | Toggle::CostPerSecond(cost) => cost,
        })
    }

    /// Whether every per-rank array it holds has an entry for each of `ranks` ranks.
    pub fn check_ranks(&self, ranks: usize) -> bool {
        self.rank_counts().all(|count| count == ranks)
    }

    /// Its capability fields at `rank`, a `{ param }` read from its params at that rank, what
    /// each name of its cost takes from by `target`: global reach and 0 for a field it lacks;
    /// the field that does not hold otherwise. A field must be a whole number of milliseconds,
    /// of a pool or of a player resource that `target` finds, or a range of meters that is not
    /// negative; and it may not read a scaling param, whose value is the caster's, not the
    /// action's.
    pub fn fields_at(
        &self,
        rank: u8,
        target: impl Fn(&DeclaredName) -> Option<CostTarget>,
    ) -> Result<RankFields, ActionField> {
        let param_at = |name: &str, field| match self.params.get(name) {
            Some(Param::Ranked(ranked)) => ranked.at(rank).ok_or(field),
            Some(Param::Scaling(_)) | None => Err(field),
        };
        let whole = |field, ranked: Option<&Ranked<Number>>| -> Result<u64, ActionField> {
            let Some(ranked) = ranked else {
                return Ok(0);
            };
            let value = match ranked.get(rank).ok_or(field)? {
                Number::Value(value) => *value,
                Number::Param(reference) => param_at(reference.param.as_str(), field)?,
            };
            match value {
                Scalar::Int(value) => u64::try_from(value).ok(),
                Scalar::Decimal(_) => None,
            }
            .ok_or(field)
        };
        let range = match &self.range {
            None => Range::Global,
            Some(ranked) => match ranked.get(rank).ok_or(ActionField::Range)? {
                RangeField::Range(range) => *range,
                RangeField::Param(reference) => {
                    let meters = param_at(reference.param.as_str(), ActionField::Range)?.to_num();
                    let meters = meters.filter(|meters| *meters >= Num::ZERO);
                    Range::Meters(meters.ok_or(ActionField::Range)?)
                }
            },
        };
        let mut cost = Vec::with_capacity(self.cost.len());
        let mut resource_cost = Vec::with_capacity(self.cost.len());
        for (name, amount) in &self.cost {
            let amount = whole(ActionField::Cost, Some(amount))?;
            let amount = i64::try_from(amount).ok().ok_or(ActionField::Cost)?;
            match target(name).ok_or(ActionField::Cost)? {
                CostTarget::Pool(pool) => {
                    cost.push((pool, Num::from_int(amount).ok_or(ActionField::Cost)?));
                }
                CostTarget::Resource(resource) => {
                    resource_cost.push(ResourceAmount { resource, amount });
                }
            }
        }
        Ok(RankFields {
            range,
            cooldown_ms: whole(ActionField::Cooldown, self.cooldown_ms.as_ref())?,
            cost: PoolCost::new(cost),
            resource_cost,
            windup_ms: whole(ActionField::Windup, self.windup_ms.as_ref())?,
        })
    }

    /// Every number field that reads a param, `{ param = "<name>" }`: the names it reads.
    pub fn param_refs(&self) -> impl Iterator<Item = &DeclaredName> + '_ {
        [
            self.cooldown_ms.as_ref(),
            self.windup_ms.as_ref(),
            self.channel.as_ref().map(|channel| &channel.duration_ms),
            self.channel.as_ref().map(|channel| &channel.tick_ms),
            self.charges.as_ref().map(|charges| &charges.max),
            self.charges.as_ref().map(|charges| &charges.recharge_ms),
            self.charge.as_ref().map(|charge| &charge.max_ms),
        ]
        .into_iter()
        .flatten()
        .chain(self.costs())
        .flat_map(Ranked::values)
        .filter_map(Number::param)
        .chain(
            self.range
                .iter()
                .flat_map(Ranked::values)
                .filter_map(|range| match range {
                    RangeField::Range(_) => None,
                    RangeField::Param(reference) => Some(&reference.param),
                }),
        )
        .chain(
            self.effects()
                .flat_map(|effect| effect.does.numbers())
                .filter_map(Number::param),
        )
    }

    /// The ids of the modifiers its data names: the one it holds, its passive, and those its
    /// effect lists apply.
    pub fn modifiers(&self) -> impl Iterator<Item = &DeclaredName> + '_ {
        let effects = self.effects().filter_map(|effect| effect.does.modifier());
        [self.hold.as_ref(), self.passive_modifier.as_ref()]
            .into_iter()
            .flatten()
            .chain(effects)
    }

    /// Its effect lists, each with the hook it runs before: `on_resolve`, `on_hit`, `on_end`.
    pub fn effect_lists(&self) -> [(Hook, &[EffectData]); 3] {
        [
            (Hook::OnResolve, &self.on_resolve[..]),
            (Hook::OnHit, &self.on_hit[..]),
            (Hook::OnEnd, &self.on_end[..]),
        ]
    }

    /// Every effect of its lists.
    fn effects(&self) -> impl Iterator<Item = &EffectData> + '_ {
        self.on_resolve
            .iter()
            .chain(&self.on_hit)
            .chain(&self.on_end)
    }

    /// The filter its data names: its targeting's.
    pub fn filters(&self) -> impl Iterator<Item = &FilterData> + '_ {
        match &self.targeting {
            Targeting::Unit(filter) => Some(filter),
            Targeting::None | Targeting::Point | Targeting::Direction => None,
        }
        .into_iter()
    }
}

/// An action's capability fields at one rank, as data gives them: times in milliseconds, its
/// cost in its caster's pools, and in its caster's player's resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankFields {
    pub range: Range,
    pub cooldown_ms: u64,
    pub cost: PoolCost,
    pub resource_cost: Vec<ResourceAmount>,
    pub windup_ms: u64,
}

/// What a name of a cost takes from: a pool of the unit, or a resource of its player; the mode's
/// pools and player resources never share a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostTarget {
    Pool(PoolId),
    Resource(ResourceId),
}

/// What an action targets. In data: `none`, `point`, `direction`, or a filter of the units it
/// may target, such as `enemies` or `enemies:avatar`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Targeting {
    None,
    Point,
    Direction,
    Unit(FilterData),
}

/// A range as data writes it: a range, or `{ param = "<name>" }`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum RangeField {
    Range(Range),
    Param(ParamRef),
}

/// How far an action reaches. In data: meters as a decimal string, or `global`.
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
            filter => FilterData::parse(filter)
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
