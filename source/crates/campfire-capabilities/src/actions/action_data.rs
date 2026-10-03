use std::collections::BTreeMap;
use std::num::NonZeroU8;

use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::actions::action_kind::ActionKind;
use crate::actions::cost_target::CostTarget;
use crate::actions::delivery_data::DeliveryData;
use crate::actions::effect_data::EffectData;
use crate::actions::error::ActionField;
use crate::actions::range::Range;
use crate::players::resource_amount::ResourceAmount;
use crate::scripts::hook::Hook;
use crate::stats::pool_cost::PoolCost;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::number::{Number, ParamRef};
use crate::values::package_path::PackagePath;
use crate::values::param::Param;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;
use crate::values::stat::Stat;

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
        let whole = |field, ranked: Option<&Ranked<Number>>| self.whole_at(rank, field, ranked);
        let range = match &self.range {
            None => Range::Global,
            Some(ranked) => match ranked.get(rank).ok_or(ActionField::Range)? {
                RangeField::Range(range) => *range,
                RangeField::Param(reference) => {
                    let meters = self
                        .param_at(rank, reference.param.as_str(), ActionField::Range)?
                        .to_num();
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
            charges: self.charges_at(rank)?,
            toggle: self.toggle_at(rank, &target)?,
            channel: self.channel_at(rank)?,
        })
    }

    /// Its param `name` at `rank`, a ranked one; `field` for one it does not declare, or a
    /// scaling one, whose value is the caster's, not the action's.
    fn param_at(&self, rank: u8, name: &str, field: ActionField) -> Result<Scalar, ActionField> {
        match self.params.get(name) {
            Some(Param::Ranked(ranked)) => ranked.at(rank).ok_or(field),
            Some(Param::Scaling(_)) | None => Err(field),
        }
    }

    /// The whole number `ranked` gives at `rank`, a `{ param }` read at that rank; 0 with none,
    /// and `field` for one that is not a whole number at least 0.
    fn whole_at(
        &self,
        rank: u8,
        field: ActionField,
        ranked: Option<&Ranked<Number>>,
    ) -> Result<u64, ActionField> {
        let Some(ranked) = ranked else {
            return Ok(0);
        };
        let value = match ranked.get(rank).ok_or(field)? {
            Number::Value(value) => *value,
            Number::Param(reference) => self.param_at(rank, reference.param.as_str(), field)?,
        };
        match value {
            Scalar::Int(value) => u64::try_from(value).ok(),
            Scalar::Decimal(_) => None,
        }
        .ok_or(field)
    }

    /// Its charges at `rank`: a count of 1 to 255, and a recharge.
    fn charges_at(&self, rank: u8) -> Result<Option<RankCharges>, ActionField> {
        let field = ActionField::Charges;
        self.charges
            .as_ref()
            .map(|charges| {
                let max = self.whole_at(rank, field, Some(&charges.max))?;
                let max = u8::try_from(max).ok().and_then(NonZeroU8::new);
                Ok(RankCharges {
                    max: max.ok_or(field)?,
                    recharge_ms: self.whole_at(rank, field, Some(&charges.recharge_ms))?,
                })
            })
            .transpose()
    }

    /// Its toggle at `rank`: its cost, in the pools `target` finds alone.
    fn toggle_at(
        &self,
        rank: u8,
        target: impl Fn(&DeclaredName) -> Option<CostTarget>,
    ) -> Result<Option<RankToggle>, ActionField> {
        let field = ActionField::Toggle;
        self.toggle
            .as_ref()
            .map(|toggle| {
                let (per, costs) = match toggle {
                    Toggle::CostPerAttack(costs) => (TogglePer::Attack, costs),
                    Toggle::CostPerSecond(costs) => (TogglePer::Second, costs),
                };
                let mut cost = Vec::with_capacity(costs.len());
                for (name, amount) in costs {
                    let amount = self.whole_at(rank, field, Some(amount))?;
                    let amount = i64::try_from(amount).ok().and_then(Num::from_int);
                    let Some(CostTarget::Pool(pool)) = target(name) else {
                        return Err(field);
                    };
                    cost.push((pool, amount.ok_or(field)?));
                }
                Ok(RankToggle {
                    per,
                    cost: PoolCost::new(cost),
                })
            })
            .transpose()
    }

    /// Its channel at `rank`: a length and a time between ticks, neither 0.
    fn channel_at(&self, rank: u8) -> Result<Option<RankChannel>, ActionField> {
        let field = ActionField::Channel;
        self.channel
            .as_ref()
            .map(|channel| {
                let duration_ms = self.whole_at(rank, field, Some(&channel.duration_ms))?;
                let tick_ms = self.whole_at(rank, field, Some(&channel.tick_ms))?;
                if duration_ms == 0 || tick_ms == 0 {
                    return Err(field);
                }
                Ok(RankChannel {
                    duration_ms,
                    tick_ms,
                })
            })
            .transpose()
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

    /// Every effect of its lists, and of the lists a launch holds, however deep.
    fn effects(&self) -> impl Iterator<Item = &EffectData> + '_ {
        let mut found = Vec::new();
        let mut open: Vec<&EffectData> = self
            .on_resolve
            .iter()
            .chain(&self.on_hit)
            .chain(&self.on_end)
            .collect();
        while let Some(effect) = open.pop() {
            open.extend(effect.does.nested());
            found.push(effect);
        }
        found.into_iter()
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
    pub charges: Option<RankCharges>,
    pub toggle: Option<RankToggle>,
    pub channel: Option<RankChannel>,
}

/// A channel at one rank: how long it runs, and the time between its ticks, both positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankChannel {
    pub duration_ms: u64,
    pub tick_ms: u64,
}

/// A toggle's cost at one rank: in the caster's pools, paid as each attack goes off, or at each
/// whole second it is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankToggle {
    pub per: TogglePer,
    pub cost: PoolCost,
}

/// When a toggle pays its cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TogglePer {
    Attack,
    Second,
}

/// An action's charges at one rank: how many it holds at most, and how long one takes to come
/// back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankCharges {
    pub max: NonZeroU8,
    pub recharge_ms: u64,
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

#[cfg(test)]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use crate::actions::action_data::{ActionData, Targeting};
    use crate::actions::action_kind::ActionKind;

    impl ActionData {
        /// A cast of `targeting` and nothing more, as a table that names its targeting alone reads.
        pub(crate) fn cast(targeting: Targeting) -> ActionData {
            ActionData {
                kind: ActionKind::default(),
                script: None,
                targeting,
                range: None,
                cooldown_ms: None,
                cost: BTreeMap::new(),
                windup_ms: None,
                clamp_to_range: false,
                toggle: None,
                channel: None,
                hold: None,
                charges: None,
                charge: None,
                passive_modifier: None,
                passive_while_ready: false,
                delivery: None,
                rate: None,
                damage: None,
                damage_kind: None,
                unit_type: None,
                params: BTreeMap::new(),
                on_resolve: Vec::new(),
                on_hit: Vec::new(),
                on_end: Vec::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cast_is_what_a_table_of_its_targeting_alone_reads() {
        let read = |text: &str| toml::from_str::<ActionData>(text);
        assert_eq!(
            read(r#"targeting = "none""#),
            Ok(ActionData::cast(Targeting::None))
        );
        assert_eq!(
            read(r#"targeting = "point""#),
            Ok(ActionData::cast(Targeting::Point))
        );
    }
}
