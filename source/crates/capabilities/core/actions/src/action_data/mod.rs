use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU16, NonZeroU32};
use std::slice;

use campfire_common::DeclaredName;
use campfire_geometry::MAX_BOUND;
use campfire_math::Num;
use campfire_players::ResourceAmount;
use campfire_script::{DataSection, DataTable};
use campfire_stats::PoolCost;
use campfire_values::{
    Distance, FilterData, MessageId, Number, PackagePath, Param, Rank, Ranked, RelationSet, Scalar,
    Share, Speed, Stat,
};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::action_data::error::{ActionField, EffectRule};
use crate::action_kind::ActionKind;
use crate::action_range::ActionRange;
use crate::build_start::BuildStart;
use crate::construct_data::ConstructData;
use crate::cost_target::CostTarget;
use crate::delivery_data::DeliveryData;
use crate::effect_data::EffectData;
use crate::list_hook::ListHook;
use crate::placement_data::PlacementData;
use crate::range_field::RangeField;
use crate::rank_fields::{RankChannel, RankCharges, RankClip, RankFields, RankToggle, TogglePer};
use crate::requires_data::RequiresData;
use crate::targeting::Targeting;

pub(crate) mod error;

/// An action as its package's `[actions.<id>]` declares it, in milliseconds. Each capability
/// field may hold one value or one per rank. The release loads every field, and runs those the
/// script API reference lists as running.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionData {
    /// The message of the name players see, which an avatar's or a loadout's action needs.
    pub name: Option<MessageId>,
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
    pub charge: Option<ChargeUpData>,
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
    /// A weapon's own damage, in place of its stat `damage`, an `attack`'s alone.
    pub amount: Option<Ranked<Number>>,
    /// A weapon's own time from one attack's start to the next, in place of its stat `rate`, an
    /// `attack`'s alone.
    pub period_ms: Option<Ranked<Number>>,
    /// A weapon's clip, an `attack`'s alone.
    pub clip: Option<ClipData>,
    /// The damage a weapon's projectile deals where it ends, an `attack`'s alone.
    pub splash: Option<SplashData>,
    /// How far from its target a weapon's projectile may aim, an `attack`'s alone.
    pub scatter: Option<ScatterData>,
    /// The pitches a weapon's target lies within, an `attack`'s alone.
    pub pitch_deg: Option<PitchData>,
    /// How fast a weapon of no projectile reaches its target, which delays its hit, an
    /// `attack`'s alone (04-capabilities/combat.md, A slow weapon delays its damage).
    pub speed: Option<Speed>,
    /// The unit type of its package a `train` makes, a train's alone.
    pub unit_type: Option<DeclaredName>,
    /// What its player must hold for a `train` to join a queue, or a `build` to start.
    pub requires: Option<RequiresData>,
    /// How a `build`'s site grows.
    pub construct: Option<ConstructData>,
    /// When a `build` starts, as its builder arrives when absent.
    pub start: Option<BuildStart>,
    /// The share of its life pool's maximum a `build`'s site starts with.
    pub start_life: Option<Share>,
    /// The share of the player resources a `build` paid that a cancel of its site returns, all
    /// when absent.
    pub cancel_refund: Option<Share>,
    /// Where a `build` may place its box.
    pub placement: Option<PlacementData>,
    /// The mode's player resource a `gather` gathers.
    pub resource: Option<DeclaredName>,
    /// The most a `gather`'s trip carries.
    pub take: Option<NonZeroU32>,
    /// How far from its node, in meters, a `gather` looks for another, none when absent.
    pub bounce: Option<Distance>,
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

/// A channel, which starts after `on_resolve` and calls `on_channel_tick` every `tick_ms`; with
/// `damage_interrupts`, damage of more than 0 to its unit cuts it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelData {
    pub duration_ms: Ranked<Number>,
    pub tick_ms: Ranked<Number>,
    #[serde(default)]
    pub damage_interrupts: bool,
}

/// An action's `charges`: the uses it stores, at most `max`, each coming back `recharge_ms`
/// after it is spent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChargesData {
    pub max: Ranked<Number>,
    pub recharge_ms: Ranked<Number>,
}

/// A weapon's `clip`: the shots it fires before it reloads, and the time it reloads for, from
/// the shot that empties it, in place of its period.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipData {
    pub shots: Ranked<Number>,
    pub reload_ms: Ranked<Number>,
}

/// A weapon's `splash` (04-capabilities/combat.md, An attack may splash): where its projectile
/// ends, its damage to each living unit of the relations `affects` names whose volume lies
/// within `radius` meters, and to its source unless `spares_source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplashData {
    pub radius: Num,
    pub affects: RelationSet,
    pub spares_source: bool,
}

/// A weapon's `scatter` (04-capabilities/combat.md, An attack may scatter): the most meters
/// from its target's ground point its projectile aims at, `radius` and, for a target of a tag
/// `tags` names, the largest of their radii more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScatterData {
    pub radius: Num,
    pub tags: BTreeMap<DeclaredName, Num>,
}

/// A weapon's `pitch_deg` (04-capabilities/combat.md, An attack may bound its pitch): the
/// least and the most angle, in degrees above the ground plane, at which its target's center
/// lies from its attacker's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitchData {
    pub low: Num,
    pub high: Num,
}

/// Data is untrusted, so a radius that is not positive or past `MAX_BOUND` fails to read.
impl<'de> Deserialize<'de> for SplashData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            radius: Num,
            affects: RelationSet,
            spares_source: Option<bool>,
        }
        let fields = Fields::deserialize(deserializer)?;
        if fields.radius <= Num::ZERO || fields.radius > MAX_BOUND {
            return Err(D::Error::custom(
                "a splash's radius is positive, up to 2,048 m",
            ));
        }
        Ok(Self {
            radius: fields.radius,
            affects: fields.affects,
            spares_source: fields.spares_source.unwrap_or(true),
        })
    }
}

/// Data is untrusted, so a radius below 0 or past `MAX_BOUND` fails to read.
impl<'de> Deserialize<'de> for ScatterData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            radius: Num,
            #[serde(default)]
            tags: BTreeMap<DeclaredName, Num>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let holds = |radius: &Num| (Num::ZERO..=MAX_BOUND).contains(radius);
        if !holds(&fields.radius) || !fields.tags.values().all(holds) {
            return Err(D::Error::custom("a scatter's radius is from 0 to 2,048 m"));
        }
        Ok(Self {
            radius: fields.radius,
            tags: fields.tags,
        })
    }
}

/// Data is untrusted, so a pitch below −90° or above 90°, or a least one past the most, fails to
/// read.
impl<'de> Deserialize<'de> for PitchData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let [low, high] = <[Num; 2]>::deserialize(deserializer)?;
        // A least of 90° or a most of −90° would hold one pitch, which the test of a bound,
        // by the sine of the pitch past it, cannot tell from the opposite one.
        let quarter = Num::int(90);
        if low < -quarter || high > quarter || low > high || low == quarter || high == -quarter {
            return Err(D::Error::custom(
                "a pitch's bound is two angles from -90 to 90 degrees, the least first and \
                 below 90, the most above -90",
            ));
        }
        Ok(Self { low, high })
    }
}

/// An action's `charge`: a cast that charges from its start, in place of its windup, and
/// resolves when its order is given again or at `max_ms`, whichever is first.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChargeUpData {
    pub max_ms: Ranked<Number>,
}

impl ActionData {
    /// The length of every per-rank array it holds: its capability fields' and its params'.
    pub fn rank_counts(&self) -> impl Iterator<Item = usize> + '_ {
        let range = self.range.as_ref().and_then(Ranked::ranks);
        let numbers = self.ranked_numbers().chain(self.costs()).map(Ranked::ranks);
        let params = self.params.values().map(Param::ranks);
        [range].into_iter().chain(numbers).chain(params).flatten()
    }

    /// Each of its capability fields that is a number at each rank, which may read a param:
    /// its cooldown, windup, channel, charges, charge, and a weapon's amount, period and clip.
    fn ranked_numbers(&self) -> impl Iterator<Item = &Ranked<Number>> {
        [
            self.cooldown_ms.as_ref(),
            self.windup_ms.as_ref(),
            self.amount.as_ref(),
            self.period_ms.as_ref(),
            self.clip.as_ref().map(|clip| &clip.shots),
            self.clip.as_ref().map(|clip| &clip.reload_ms),
            self.channel.as_ref().map(|channel| &channel.duration_ms),
            self.channel.as_ref().map(|channel| &channel.tick_ms),
            self.charges.as_ref().map(|charges| &charges.max),
            self.charges.as_ref().map(|charges| &charges.recharge_ms),
            self.charge.as_ref().map(|charge| &charge.max_ms),
        ]
        .into_iter()
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

    /// The rule `number`, a number of one of its effects, breaks: a value at some rank, its own or
    /// its param's, past what a sim number holds, or below 0. A param it does not declare gives
    /// none, which its effects' names refuse.
    pub(crate) fn number_rule(&self, number: &Number) -> Result<(), EffectRule> {
        let (value, param) = match number {
            Number::Value(value) => (Some(value.to_num()), None),
            Number::Param(reference) => (None, self.params.get(&reference.param)),
        };
        let values = value
            .into_iter()
            .chain(param.into_iter().flat_map(Param::numbers));
        for value in values {
            if value.ok_or(EffectRule::Overflow)? < Num::ZERO {
                return Err(EffectRule::Negative);
            }
        }
        Ok(())
    }

    /// Whether `duration`, a duration of one of its effects, is whole milliseconds within a
    /// `u32` at every rank, and reads no scaling param, whose value only a call knows.
    pub(crate) fn whole_ms(&self, duration: &Number) -> bool {
        let values = match duration {
            Number::Value(value) => slice::from_ref(value),
            Number::Param(reference) => match self.params.get(&reference.param) {
                Some(Param::Ranked(ranked)) => ranked.values(),
                Some(Param::Scaling(_)) => return false,
                None => &[],
            },
        };
        let whole = |value: &Scalar| matches!(value, Scalar::Int(ms) if u32::try_from(*ms).is_ok());
        values.iter().all(whole)
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
        rank: Rank,
        target: impl Fn(&DeclaredName) -> Option<CostTarget>,
    ) -> Result<RankFields, ActionField> {
        let whole = |field, ranked: Option<&Ranked<Number>>| self.whole_at(rank, field, ranked);
        let range = match &self.range {
            None => ActionRange::Global,
            Some(ranked) => match ranked.get(rank).ok_or(ActionField::Range)? {
                RangeField::Range(range) => *range,
                RangeField::Param(reference) => {
                    let meters = self
                        .param_at(rank, reference.param.as_str(), ActionField::Range)?
                        .to_num();
                    let meters = meters.filter(|meters| *meters >= Num::ZERO);
                    ActionRange::Meters(meters.ok_or(ActionField::Range)?)
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
            charge_ms: self.charge_at(rank)?,
            amount: self.amount_at(rank)?,
            period_ms: self.period_at(rank)?,
            clip: self.clip_at(rank)?,
        })
    }

    /// A weapon's own damage at `rank`: a number at least 0.
    fn amount_at(&self, rank: Rank) -> Result<Option<Num>, ActionField> {
        let field = ActionField::Amount;
        self.amount
            .as_ref()
            .map(|ranked| {
                let value = match ranked.get(rank).ok_or(field)? {
                    Number::Value(value) => *value,
                    Number::Param(reference) => {
                        self.param_at(rank, reference.param.as_str(), field)?
                    }
                };
                let amount = value.to_num().filter(|amount| *amount >= Num::ZERO);
                amount.ok_or(field)
            })
            .transpose()
    }

    /// A weapon's own period at `rank`: whole milliseconds, not 0.
    fn period_at(&self, rank: Rank) -> Result<Option<u64>, ActionField> {
        let field = ActionField::Period;
        self.period_ms
            .as_ref()
            .map(|ranked| {
                let period = self.whole_at(rank, field, Some(ranked))?;
                (period > 0).then_some(period).ok_or(field)
            })
            .transpose()
    }

    /// A weapon's clip at `rank`: shots of 1 to 65,535, and a reload.
    fn clip_at(&self, rank: Rank) -> Result<Option<RankClip>, ActionField> {
        let field = ActionField::Clip;
        self.clip
            .as_ref()
            .map(|clip| {
                let shots = self.whole_at(rank, field, Some(&clip.shots))?;
                let shots = u16::try_from(shots).ok().and_then(NonZeroU16::new);
                Ok(RankClip {
                    shots: shots.ok_or(field)?,
                    reload_ms: self.whole_at(rank, field, Some(&clip.reload_ms))?,
                })
            })
            .transpose()
    }

    /// Its param `name` at `rank`, a ranked one; `field` for one it does not declare, or a
    /// scaling one, whose value is the caster's, not the action's.
    fn param_at(&self, rank: Rank, name: &str, field: ActionField) -> Result<Scalar, ActionField> {
        match self.params.get(name) {
            Some(Param::Ranked(ranked)) => ranked.get(rank).copied().ok_or(field),
            Some(Param::Scaling(_)) | None => Err(field),
        }
    }

    /// The whole number `ranked` gives at `rank`, a `{ param }` read at that rank; 0 with none,
    /// and `field` for one that is not a whole number at least 0.
    fn whole_at(
        &self,
        rank: Rank,
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
    fn charges_at(&self, rank: Rank) -> Result<Option<RankCharges>, ActionField> {
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
        rank: Rank,
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

    /// Its charge's most at `rank`: whole milliseconds, not 0.
    fn charge_at(&self, rank: Rank) -> Result<Option<u64>, ActionField> {
        let field = ActionField::Charge;
        self.charge
            .as_ref()
            .map(|charge| {
                let most = self.whole_at(rank, field, Some(&charge.max_ms))?;
                (most > 0).then_some(most).ok_or(field)
            })
            .transpose()
    }

    /// Its channel at `rank`: a length and a time between ticks, neither 0, and whether damage
    /// cuts it.
    fn channel_at(&self, rank: Rank) -> Result<Option<RankChannel>, ActionField> {
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
                    damage_interrupts: channel.damage_interrupts,
                })
            })
            .transpose()
    }

    /// Every number field that reads a param, `{ param = "<name>" }`: the names it reads.
    pub fn param_refs(&self) -> impl Iterator<Item = &DeclaredName> + '_ {
        self.ranked_numbers()
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
    pub const fn effect_lists(&self) -> [(ListHook, &[EffectData]); 3] {
        [
            (ListHook::OnResolve, self.on_resolve.as_slice()),
            (ListHook::OnHit, self.on_hit.as_slice()),
            (ListHook::OnEnd, self.on_end.as_slice()),
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

impl DataSection for ActionData {
    const TABLE: DataTable = DataTable::Action;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use crate::action_data::ActionData;
    use crate::action_kind::ActionKind;
    use crate::targeting::Targeting;

    impl ActionData {
        /// A cast of `targeting` and nothing more, as a table that names its targeting alone reads.
        pub fn cast(targeting: Targeting) -> Self {
            Self {
                name: None,
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
                amount: None,
                period_ms: None,
                clip: None,
                splash: None,
                scatter: None,
                pitch_deg: None,
                speed: None,
                unit_type: None,
                requires: None,
                construct: None,
                start: None,
                start_life: None,
                cancel_refund: None,
                placement: None,
                resource: None,
                take: None,
                bounce: None,
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
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_cast_is_what_a_table_of_its_targeting_alone_reads() {
        let read = |text: &str| Toml::parse::<ActionData>(text);
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
