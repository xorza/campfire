use std::any::Any;
use std::collections::BTreeMap;
use std::num::NonZeroU8;
use std::slice;
use std::sync::Arc;

use campfire_codec::{StructFields, TomlError, TomlTable, TomlValue};
use campfire_common::{DeclaredName, Ticks};
use campfire_declarations::SectionReaders;
use campfire_math::Num;
use campfire_players::ResourceAmount;
use campfire_script::DataTable;
use campfire_sim::MatchRate;
use campfire_units::PoolCost;
use campfire_values::{
    Distance, FilterData, Number, PackagePath, Param, Rank, Ranked, Scalar, TagFilterData, TextId,
    TimeTooLarge,
};
use serde::de::Error;
use serde::{Deserialize, Serialize};

use crate::action_kind::ActionKind;
use crate::action_range::ActionRange;
use crate::cooldown_holder::CooldownHolder;
use crate::cost_target::CostTarget;
use crate::delivery_section::{DeliverySection, DeliverySections};
use crate::error::{ActionField, ActionProblem};
use crate::kind_section::{KindSection, KindSections};
use crate::kind_takes::Takes;
use crate::list_hook::ListHook;
use crate::listed_effect::effect_data::EffectData;
use crate::listed_effect::error::EffectRule;
use crate::pipeline_field::PipelineField;
use crate::range_field::RangeField;
use crate::rank_toggle::{RankToggle, TogglePer};
use crate::rank_values::{ChannelRule, ChargeRule, CostRun, LoadedRanks, RankValues};
use crate::targeting::Targeting;

/// An action as its package's `[actions.<id>]` declares it, in milliseconds: the pipeline's
/// keys, each capability field one value or one per rank, its delivery, its kind's section, which
/// reads the rest of its table (04-capabilities/actions.md, Each kind owns its section), and its
/// effect lists.
#[derive(Debug, Clone)]
pub struct ActionData {
    pub keys: ActionKeys,
    /// How it reaches what it affects, other than at once, as the module that registers the
    /// delivery's form reads it.
    pub delivery: Option<Arc<dyn DeliverySection>>,
    /// Its kind, and the fields of its kind's own, as the module that registers its kind reads
    /// them.
    pub section: Arc<dyn KindSection>,
    /// The effects of its resolve, which queue before its script's `on_resolve`.
    pub on_resolve: Vec<EffectData>,
    /// The effects of each hit of its delivery, which queue before `on_hit`.
    pub on_hit: Vec<EffectData>,
    /// The effects of its delivery's end, which queue before `on_end`.
    pub on_end: Vec<EffectData>,
}

// By hand, as a derive compares the trait objects by a move.
impl PartialEq for ActionData {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            keys,
            delivery,
            section,
            on_resolve,
            on_hit,
            on_end,
        } = self;
        *keys == other.keys
            && *delivery == other.delivery
            && **section == *other.section
            && *on_resolve == other.on_resolve
            && *on_hit == other.on_hit
            && *on_end == other.on_end
    }
}

impl Eq for ActionData {}

/// The keys of an action's table that the pipeline reads, every kind's, but its kind's name,
/// which its section gives, its delivery and its lists, which read with the release's readers. A
/// tool writes an action through them, beside its kind's section (03-game-scripting.md, A section
/// has a partial form).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionKeys {
    /// The message of the name players see, which an avatar's or a loadout's action needs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<TextId>,
    /// The script, if the action needs one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<PackagePath>,
    pub targeting: Targeting,
    /// The tags its unit must pass for it to start, for its passive to hold, and for an attack to
    /// be chosen as a weapon; it keeps its slot whatever they are (04-capabilities/actions.md, An
    /// action may need tags).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<TagFilterData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Ranked<RangeField>>,
    /// The meters nearer than which it takes no target (04-capabilities/actions.md, An action may
    /// keep a minimum range).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_range: Option<Ranked<Number>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_ms: Option<Ranked<Number>>,
    /// In each pool of the caster it names, and each resource of the caster's player.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cost: BTreeMap<DeclaredName, Ranked<Number>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windup_ms: Option<Ranked<Number>>,
    /// A target beyond range is moved in, instead of the caster walking.
    #[serde(default, skip_serializing_if = "is_off")]
    pub clamp_to_range: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggle: Option<Toggle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<ChannelData>,
    /// The modifier held while the toggle is on or the channel runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<DeclaredName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charges: Option<ChargesData>,
    /// A charged cast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charging: Option<ChargeUpData>,
    /// The modifier held while the action has a rank.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passive_modifier: Option<DeclaredName>,
    /// The passive modifier is held only while the action is off cooldown.
    #[serde(default, skip_serializing_if = "is_off")]
    pub passive_while_ready: bool,
    /// Who keeps its cooldown, its unit by default.
    #[serde(default, skip_serializing_if = "CooldownHolder::is_unit")]
    pub cooldown_holder: CooldownHolder,
    /// Values for the script, as `ctx.params` reads them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<DeclaredName, Param>,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's test of a field to skip takes it by reference"
)]
const fn is_off(flag: &bool) -> bool {
    !*flag
}

impl ActionKeys {
    /// The keys of an action that targets as `targeting`, and gives no other key.
    pub const fn new(targeting: Targeting) -> Self {
        Self {
            name: None,
            script: None,
            targeting,
            when: None,
            range: None,
            min_range: None,
            cooldown_ms: None,
            cost: BTreeMap::new(),
            windup_ms: None,
            clamp_to_range: false,
            toggle: None,
            channel: None,
            hold: None,
            charges: None,
            charging: None,
            passive_modifier: None,
            passive_while_ready: false,
            cooldown_holder: CooldownHolder::Unit,
            params: BTreeMap::new(),
        }
    }

    /// The table of the action of these keys, `section`, the fields of its kind's own, and its
    /// `delivery`, as `ActionData::read` takes them apart.
    pub fn table_delivering<K: KindSection + Serialize, D: DeliverySection + Serialize>(
        &self,
        section: &K,
        delivery: &D,
    ) -> TomlTable {
        let mut table = self.table(section);
        table.insert(ActionData::DELIVERY.to_owned(), TomlValue::of(delivery));
        table
    }

    /// The table of the action of these keys and `section`, the fields of its kind's own, side
    /// by side, as `ActionData::read` takes them apart.
    pub fn table<K: KindSection + Serialize>(&self, section: &K) -> TomlTable {
        let mut table: TomlTable = TomlValue::of(self).read().expect("the keys write a table");
        if section.kind() != ActionKind::default() {
            table.insert(ActionData::KIND.to_owned(), TomlValue::of(&section.kind()));
        }
        let fields: TomlTable = TomlValue::of(section)
            .read()
            .expect("a kind's section writes a table");
        for (key, value) in fields.into_entries() {
            debug_assert!(
                !table.holds(&key),
                "`{key}` is the pipeline's or the kind's"
            );
            table.insert(key, value);
        }
        table
    }
}

/// A toggle's cost, in each pool of the caster it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Toggle {
    CostPerAttack(BTreeMap<DeclaredName, Ranked<Number>>),
    CostPerSecond(BTreeMap<DeclaredName, Ranked<Number>>),
}

/// A channel, which starts after `on_resolve` and calls `on_channel_tick` every `tick_ms`, for
/// `duration_ms`, or with none until an order, a stop, its unit's death or a tag ends it; with
/// `damage_interrupts`, damage of more than 0 to its unit cuts it. Its cast's windup spreads by
/// `windup_spread` each way, a fraction below 1 drawn as the cast starts; an order that cuts it
/// waits `wind_down_ms`, and the last given then runs; each tick pays its unit's owner `pays`
/// (04-capabilities/actions.md, A channel with no end).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelData {
    #[serde(default)]
    pub duration_ms: Option<Ranked<Number>>,
    pub tick_ms: Ranked<Number>,
    #[serde(default)]
    pub damage_interrupts: bool,
    #[serde(default)]
    pub windup_spread: Option<Ranked<Number>>,
    #[serde(default)]
    pub wind_down_ms: Option<Ranked<Number>>,
    #[serde(default)]
    pub pays: Option<ChannelPaysData>,
}

/// What each tick of a channel pays its unit's owner: `amount` of `resource`, by the unit's level
/// on `track`, from level 1, the last for each level past them; with no track, one amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelPaysData {
    pub resource: DeclaredName,
    #[serde(default)]
    pub track: Option<DeclaredName>,
    pub amount: Vec<u32>,
}

/// An action's `charges`: the uses it stores, at most `max`, each coming back `recharge_ms`
/// after it is spent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChargesData {
    pub max: Ranked<Number>,
    pub recharge_ms: Ranked<Number>,
}

/// An action's `charge`: a cast that charges from its start, in place of its windup, and
/// resolves when its order is given again or at `max_ms`, whichever is first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChargeUpData {
    pub max_ms: Ranked<Number>,
}

impl ActionData {
    /// The table of an action's fields, which the pipeline and each kind's module give.
    pub const TABLE: DataTable = DataTable::new("An action, `[actions.<id>]`");

    /// The length of every per-rank array it holds: its capability fields' and its params'.
    pub fn rank_counts(&self) -> impl Iterator<Item = usize> + '_ {
        let range = self.keys.range.as_ref().and_then(Ranked::ranks);
        let numbers = self.ranked_numbers().chain(self.costs());
        let numbers = numbers.chain(self.section.numbers()).map(Ranked::ranks);
        let params = self.keys.params.values().map(Param::ranks);
        [range].into_iter().chain(numbers).chain(params).flatten()
    }

    /// Each of the pipeline's fields it gives that is a number at each rank, which may read a
    /// param: its minimum range, cooldown, windup, channel, charges and charge.
    fn ranked_numbers(&self) -> impl Iterator<Item = &Ranked<Number>> {
        [
            self.keys.min_range.as_ref(),
            self.keys.cooldown_ms.as_ref(),
            self.keys.windup_ms.as_ref(),
            self.keys
                .channel
                .as_ref()
                .and_then(|channel| channel.duration_ms.as_ref()),
            self.keys.channel.as_ref().map(|channel| &channel.tick_ms),
            self.keys
                .channel
                .as_ref()
                .and_then(|channel| channel.windup_spread.as_ref()),
            self.keys
                .channel
                .as_ref()
                .and_then(|channel| channel.wind_down_ms.as_ref()),
            self.keys.charges.as_ref().map(|charges| &charges.max),
            self.keys
                .charges
                .as_ref()
                .map(|charges| &charges.recharge_ms),
            self.keys.charging.as_ref().map(|charge| &charge.max_ms),
        ]
        .into_iter()
        .flatten()
    }

    /// Every pool or player resource it costs something in, its toggle's among them.
    pub fn cost_names(&self) -> impl Iterator<Item = &DeclaredName> + '_ {
        self.keys
            .cost
            .keys()
            .chain(self.toggle_cost().into_iter().flat_map(BTreeMap::keys))
    }

    /// Each amount of its cost and its toggle's, in its pools.
    fn costs(&self) -> impl Iterator<Item = &Ranked<Number>> + '_ {
        self.keys
            .cost
            .values()
            .chain(self.toggle_cost().into_iter().flat_map(BTreeMap::values))
    }

    fn toggle_cost(&self) -> Option<&BTreeMap<DeclaredName, Ranked<Number>>> {
        self.keys.toggle.as_ref().map(|toggle| match toggle {
            Toggle::CostPerAttack(cost) | Toggle::CostPerSecond(cost) => cost,
        })
    }

    /// The rule `number`, a number of one of its effects, breaks: a value at some rank, its own or
    /// its param's, past what a sim number holds, or below 0. A param it does not declare gives
    /// none, which its effects' names refuse.
    pub(crate) fn number_rule(&self, number: &Number) -> Result<(), EffectRule> {
        let (value, param) = match number {
            Number::Value(value) => (Some(value.to_num()), None),
            Number::Param(reference) => (None, self.keys.params.get(&reference.param)),
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
            Number::Param(reference) => match self.keys.params.get(&reference.param) {
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

    /// Its fields at each of its `ranks` ranks, times in ticks at `rate`, what its cost's names
    /// take from by `target`; an error for a per-rank array of another length, a
    /// field that does not hold at a rank, a use's, a build's or a gather's range that is global
    /// at a rank, a minimum range not within a range of meters at a rank, or a time that does not
    /// count in ticks.
    pub(crate) fn load_ranks(
        &self,
        ranks: u8,
        rate: MatchRate,
        target: impl Fn(&DeclaredName) -> Option<CostTarget>,
    ) -> Result<LoadedRanks, ActionProblem> {
        if !self.check_ranks(usize::from(ranks)) {
            return Err(ActionProblem::RankCount(ranks));
        }
        let metered = self.section.takes().metered();
        let mut loaded = LoadedRanks {
            values: Vec::with_capacity(usize::from(ranks)),
            resource_costs: Vec::new(),
        };
        for rank in (1..=ranks).filter_map(Rank::new) {
            let values = self.rank_values(rank, rate, &target, &mut loaded.resource_costs)?;
            if metered && values.range == ActionRange::Global {
                return Err(ActionProblem::Global);
            }
            let within = |min: Distance| match values.range {
                ActionRange::Meters(range) => min < range,
                ActionRange::Global => false,
            };
            if values.min_range.is_some_and(|min| !within(min)) {
                return Err(ActionProblem::MinRange);
            }
            loaded.values.push(values);
        }
        Ok(loaded)
    }

    /// Its capability fields at `rank`, a `{ param }` read from its params at that rank, what
    /// each name of its cost takes from by `target`, times in ticks at `rate`: global reach and 0
    /// for a field it lacks; its costs in player resources pushed onto `resource_costs`, whose
    /// run it holds. A field must be a whole number of milliseconds that counts in ticks, of a
    /// pool or of a player resource that `target` finds, or a range of meters that is not
    /// negative; and it may not read a scaling param, whose value is the caster's, not the
    /// action's.
    pub(crate) fn rank_values(
        &self,
        rank: Rank,
        rate: MatchRate,
        target: impl Fn(&DeclaredName) -> Option<CostTarget>,
        resource_costs: &mut Vec<ResourceAmount>,
    ) -> Result<RankValues, ActionProblem> {
        let whole = |field, ranked: Option<&Ranked<Number>>| self.whole(rank, field, ranked);
        let ticks = |field, ranked: Option<&Ranked<Number>>| {
            Ok::<_, ActionProblem>(rate.ticks(whole(field, ranked)?).ok_or(TimeTooLarge)?)
        };
        let range = match &self.keys.range {
            None => ActionRange::Global,
            Some(ranked) => match ranked.get(rank).ok_or(ActionField::Range)? {
                RangeField::Range(range) => *range,
                RangeField::Param(reference) => {
                    let meters = self.param_at(rank, reference.param.as_str());
                    let meters = meters.ok_or(ActionField::Range)?.to_num();
                    let meters = meters.and_then(Distance::new);
                    ActionRange::Meters(meters.ok_or(ActionField::Range)?)
                }
            },
        };
        let mut cost = Vec::with_capacity(self.keys.cost.len());
        let start = resource_costs.len();
        for (name, amount) in &self.keys.cost {
            let amount = whole(ActionField::Cost, Some(amount))?;
            let amount = i64::try_from(amount).ok().ok_or(ActionField::Cost)?;
            match target(name).ok_or(ActionField::Cost)? {
                CostTarget::Pool(pool) => {
                    cost.push((pool, Num::from_int(amount).ok_or(ActionField::Cost)?));
                }
                CostTarget::Resource(resource) => {
                    resource_costs.push(ResourceAmount { resource, amount });
                }
            }
        }
        let min_range = self.keys.min_range.as_ref().map(|ranked| {
            let meters = self.value_at(rank, ranked).and_then(Scalar::to_num);
            meters.and_then(Distance::new).ok_or(ActionField::MinRange)
        });
        Ok(RankValues {
            range,
            min_range: min_range.transpose()?,
            cooldown: ticks(ActionField::Cooldown, self.keys.cooldown_ms.as_ref())?,
            cost: PoolCost::new(cost),
            windup: ticks(ActionField::Windup, self.keys.windup_ms.as_ref())?,
            charges: self.charges_at(rank, rate)?,
            toggle: self.toggle_at(rank, &target)?,
            channel: self.channel_at(rank, rate)?,
            charge: self.charge_at(rank, rate)?,
            resource_cost: CostRun::new(start, resource_costs.len()),
        })
    }

    /// The value `ranked` gives at `rank`, a `{ param }` read from its params at that rank; none
    /// for a rank it lacks, or a param it does not declare or that scales, whose value is the
    /// caster's, not the action's. A kind's section reads its own numbers by it.
    pub fn value_at(&self, rank: Rank, ranked: &Ranked<Number>) -> Option<Scalar> {
        match ranked.get(rank)? {
            Number::Value(value) => Some(*value),
            Number::Param(reference) => self.param_at(rank, reference.param.as_str()),
        }
    }

    /// The whole number at least 0 that `ranked` gives at `rank`, as `value_at` reads it; none
    /// for another value.
    pub fn whole_at(&self, rank: Rank, ranked: &Ranked<Number>) -> Option<u64> {
        match self.value_at(rank, ranked)? {
            Scalar::Int(value) => u64::try_from(value).ok(),
            Scalar::Decimal(_) => None,
        }
    }

    /// Its param `name` at `rank`, a ranked one; none for one it does not declare, or a scaling
    /// one.
    fn param_at(&self, rank: Rank, name: &str) -> Option<Scalar> {
        match self.keys.params.get(name) {
            Some(Param::Ranked(ranked)) => ranked.get(rank).copied(),
            Some(Param::Scaling(_)) | None => None,
        }
    }

    /// The whole number `ranked` gives at `rank`, as `whole_at` reads it; 0 with none, and
    /// `field` for one that is not a whole number at least 0.
    fn whole(
        &self,
        rank: Rank,
        field: ActionField,
        ranked: Option<&Ranked<Number>>,
    ) -> Result<u64, ActionField> {
        ranked.map_or(Ok(0), |ranked| self.whole_at(rank, ranked).ok_or(field))
    }

    /// Its charges at `rank`: a count of 1 to 255, and a recharge.
    fn charges_at(&self, rank: Rank, rate: MatchRate) -> Result<Option<ChargeRule>, ActionProblem> {
        let field = ActionField::Charges;
        self.keys
            .charges
            .as_ref()
            .map(|charges| {
                let max = self.whole(rank, field, Some(&charges.max))?;
                let max = u8::try_from(max).ok().and_then(NonZeroU8::new);
                let recharge = self.whole(rank, field, Some(&charges.recharge_ms))?;
                Ok(ChargeRule {
                    max: max.ok_or(field)?,
                    recharge: rate.ticks(recharge).ok_or(TimeTooLarge)?,
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
        self.keys
            .toggle
            .as_ref()
            .map(|toggle| {
                let (per, costs) = match toggle {
                    Toggle::CostPerAttack(costs) => (TogglePer::Attack, costs),
                    Toggle::CostPerSecond(costs) => (TogglePer::Second, costs),
                };
                let mut cost = Vec::with_capacity(costs.len());
                for (name, amount) in costs {
                    let amount = self.whole(rank, field, Some(amount))?;
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
    fn charge_at(&self, rank: Rank, rate: MatchRate) -> Result<Option<Ticks>, ActionProblem> {
        let field = ActionField::Charge;
        self.keys
            .charging
            .as_ref()
            .map(|charge| {
                let most = self.whole(rank, field, Some(&charge.max_ms))?;
                let most = (most > 0).then_some(most).ok_or(field)?;
                Ok(rate.ticks(most).ok_or(TimeTooLarge)?)
            })
            .transpose()
    }

    /// Its channel at `rank`: a length, when it has one, a time between ticks and a wind-down,
    /// when it has one, none of them 0, a windup spread of 0 to below 1, and whether damage cuts
    /// it.
    fn channel_at(
        &self,
        rank: Rank,
        rate: MatchRate,
    ) -> Result<Option<ChannelRule>, ActionProblem> {
        let field = ActionField::Channel;
        self.keys
            .channel
            .as_ref()
            .map(|channel| {
                let ticks = |ranked: &Ranked<Number>| {
                    let ms = self.whole(rank, field, Some(ranked))?;
                    let ms = (ms > 0).then_some(ms).ok_or(field)?;
                    Ok::<_, ActionProblem>(rate.ticks(ms).ok_or(TimeTooLarge)?)
                };
                let spread = channel.windup_spread.as_ref().map(|ranked| {
                    let spread = self.value_at(rank, ranked).and_then(Scalar::to_num);
                    spread
                        .filter(|spread| (Num::ZERO..Num::ONE).contains(spread))
                        .ok_or(field)
                });
                Ok(ChannelRule {
                    duration: channel.duration_ms.as_ref().map(ticks).transpose()?,
                    tick: ticks(&channel.tick_ms)?,
                    damage_interrupts: channel.damage_interrupts,
                    windup_spread: spread.transpose()?,
                    wind_down: channel.wind_down_ms.as_ref().map(ticks).transpose()?,
                })
            })
            .transpose()
    }

    /// Every number field that reads a param, `{ param = "<name>" }`: the names it reads.
    pub fn param_refs(&self) -> impl Iterator<Item = &DeclaredName> + '_ {
        self.ranked_numbers()
            .chain(self.costs())
            .chain(self.section.numbers())
            .flat_map(Ranked::values)
            .filter_map(Number::param)
            .chain(self.keys.range.iter().flat_map(Ranked::values).filter_map(
                |range| match range {
                    RangeField::Range(_) => None,
                    RangeField::Param(reference) => Some(&reference.param),
                },
            ))
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
        [self.keys.hold.as_ref(), self.keys.passive_modifier.as_ref()]
            .into_iter()
            .flatten()
            .chain(effects)
    }

    /// The keys of its effect lists.
    pub const LISTS: [&'static str; 3] = ["on_resolve", "on_hit", "on_end"];

    /// The key of its delivery, whose form its module reads.
    pub const DELIVERY: &'static str = "delivery";

    /// The key of its kind's name, which picks the reader of its section.
    pub const KIND: &'static str = "kind";

    /// Its effect lists, each with the hook it runs before: `on_resolve`, `on_hit`, `on_end`.
    pub const fn effect_lists(&self) -> [(ListHook, &[EffectData]); 3] {
        [
            (ListHook::OnResolve, self.on_resolve.as_slice()),
            (ListHook::OnHit, self.on_hit.as_slice()),
            (ListHook::OnEnd, self.on_end.as_slice()),
        ]
    }

    /// Every effect of its lists, and of the lists an effect holds, however deep.
    pub fn effects(&self) -> impl Iterator<Item = &EffectData> + '_ {
        let mut found = Vec::new();
        let mut open: Vec<&EffectData> = self
            .on_resolve
            .iter()
            .chain(&self.on_hit)
            .chain(&self.on_end)
            .collect();
        while let Some(effect) = open.pop() {
            if let Some(lists) = effect.does.lists() {
                open.extend(lists.on_hit.iter().chain(&lists.on_end));
            }
            found.push(effect);
        }
        found.into_iter()
    }

    /// The keys of an action's table that the pipeline reads, every kind's, its lists among
    /// them; its kind's section reads the rest.
    pub fn pipeline_keys() -> impl Iterator<Item = &'static str> {
        let keys = StructFields::of::<ActionKeys>().iter().copied();
        keys.chain(Self::LISTS).chain([Self::DELIVERY, Self::KIND])
    }

    /// The action `table` gives: its lists each read with the release's `readers`, the keys of
    /// the pipeline, and the rest of its table as the section of its kind, which the module that
    /// registers the kind reads; an error for a kind no module registers.
    pub(crate) fn read(mut table: TomlTable, readers: &SectionReaders) -> Result<Self, TomlError> {
        let mut list = |key| {
            table
                .take_value(key)
                .map_or(Ok(Vec::new()), |list| EffectData::read_list(list, readers))
        };
        let [on_resolve, on_hit, on_end] = Self::LISTS.map(&mut list);
        let delivery = table.take_value(Self::DELIVERY);
        let delivery = delivery.map(|delivery| Self::read_delivery(delivery, readers));
        let kind = table
            .take_value(Self::KIND)
            .map(TomlValue::read)
            .transpose()?;
        let kind: ActionKind = kind.unwrap_or_default();
        let keys = table.take_keys(StructFields::of::<ActionKeys>().iter().copied());
        let keys: ActionKeys = keys.read()?;
        let Some(read) = readers.listed::<KindSections>(kind.name()) else {
            return Err(TomlError::custom(format_args!(
                "the release runs no action of the kind `{}`",
                kind.name()
            )));
        };
        let section = read(TomlValue::from(table), readers)?;
        debug_assert_eq!(
            section.kind(),
            kind,
            "a kind's section is registered by its kind's name"
        );
        Ok(Self {
            keys,
            delivery: delivery.transpose()?,
            section,
            on_resolve: on_resolve?,
            on_hit: on_hit?,
            on_end: on_end?,
        })
    }

    /// The delivery `value` gives, as the module whose form's key it names reads it: an error for
    /// a delivery that names no form a module registers, or two.
    fn read_delivery(
        value: TomlValue,
        readers: &SectionReaders,
    ) -> Result<Arc<dyn DeliverySection>, TomlError> {
        let table: TomlTable = value.clone().read()?;
        let mut named = readers
            .listed_keys::<DeliverySections>()
            .filter(|key| table.holds(key));
        let (Some(key), None) = (named.next(), named.next()) else {
            let forms: Vec<&str> = readers.listed_keys::<DeliverySections>().collect();
            return Err(TomlError::custom(format_args!(
                "a delivery names one of {}",
                forms.join(", ")
            )));
        };
        let read = readers
            .listed::<DeliverySections>(key)
            .expect("a listed key has its read");
        read(value, readers)
    }

    /// Its delivery, when its form is a `D`.
    pub fn delivery<D: DeliverySection>(&self) -> Option<&D> {
        let delivery = self.delivery.as_deref()?;
        (delivery as &dyn Any).downcast_ref::<D>()
    }

    /// Whether it gives `field`: a value, a list or a table that is not empty, or a flag that is
    /// on.
    fn gives(&self, field: PipelineField) -> bool {
        let keys = &self.keys;
        match field {
            PipelineField::Script => keys.script.is_some(),
            PipelineField::Range => keys.range.is_some(),
            PipelineField::MinRange => keys.min_range.is_some(),
            PipelineField::CooldownMs => keys.cooldown_ms.is_some(),
            PipelineField::Cost => !keys.cost.is_empty(),
            PipelineField::WindupMs => keys.windup_ms.is_some(),
            PipelineField::ClampToRange => keys.clamp_to_range,
            PipelineField::Toggle => keys.toggle.is_some(),
            PipelineField::Channel => keys.channel.is_some(),
            PipelineField::Hold => keys.hold.is_some(),
            PipelineField::Charges => keys.charges.is_some(),
            PipelineField::Charging => keys.charging.is_some(),
            PipelineField::PassiveModifier => keys.passive_modifier.is_some(),
            PipelineField::PassiveWhileReady => keys.passive_while_ready,
            PipelineField::CooldownHolder => keys.cooldown_holder == CooldownHolder::Player,
            PipelineField::Delivery => self.delivery.is_some(),
            PipelineField::Params => !keys.params.is_empty(),
            PipelineField::OnResolve => !self.on_resolve.is_empty(),
            PipelineField::OnHit => !self.on_hit.is_empty(),
            PipelineField::OnEnd => !self.on_end.is_empty(),
        }
    }

    /// The first field of the pipeline it gives that its kind never takes, or does not give that
    /// its kind always takes.
    pub fn misused(&self) -> Option<PipelineField> {
        let takes = self.section.takes();
        PipelineField::ALL
            .iter()
            .copied()
            .find(|&field| match takes.of(field) {
                Takes::Never => self.gives(field),
                Takes::May => false,
                Takes::Must => !self.gives(field),
            })
    }

    /// Its kind, as its section gives it.
    pub fn kind(&self) -> ActionKind {
        self.section.kind()
    }

    /// Its kind's section, when its kind's is an `S`.
    pub fn section<S: KindSection>(&self) -> Option<&S> {
        (&*self.section as &dyn Any).downcast_ref::<S>()
    }

    /// The filter its data names: its targeting's.
    pub fn filters(&self) -> impl Iterator<Item = &FilterData> + '_ {
        match &self.keys.targeting {
            Targeting::Unit(filter) => Some(filter),
            Targeting::None | Targeting::Point | Targeting::Direction => None,
        }
        .into_iter()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::sync::Arc;

    use crate::action_data::{ActionData, ActionKeys};
    use crate::kind_section::KindSection;
    use crate::targeting::Targeting;

    impl ActionData {
        /// An action of `section`'s kind, of `targeting` and nothing more, as a table that names
        /// its kind and its targeting alone reads.
        pub fn of(targeting: Targeting, section: impl KindSection) -> Self {
            Self {
                keys: ActionKeys::new(targeting),
                delivery: None,
                section: Arc::new(section),
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
    use crate::kind_section::tests::TestKind;
    use std::num::NonZeroU32;

    use campfire_units::PoolId;
    use campfire_values::ResourceId;

    use crate::kind_takes::KindTakes;
    use campfire_common::NamedEnum;

    fn read(text: &str) -> Result<ActionData, TomlError> {
        ActionData::read(Toml::parse(text)?, &TestKind::readers())
    }

    #[test]
    fn a_table_reads_its_pipeline_keys_and_gives_the_rest_to_its_kind() {
        assert_eq!(
            read(r#"targeting = "none""#),
            Ok(ActionData::of(Targeting::None, TestKind::default()))
        );
        let read_most = read("targeting = \"point\"\ncooldown_ms = 5\nmost = [1, 2]").unwrap();
        assert_eq!(
            read_most.keys.cooldown_ms,
            Some(Ranked::One(Number::Value(Scalar::Int(5))))
        );
        let most = Ranked::PerRank(vec![
            Number::Value(Scalar::Int(1)),
            Number::Value(Scalar::Int(2)),
        ]);
        assert_eq!(read_most.section::<TestKind>().unwrap().most, Some(most));
        // The kind reads what the pipeline does not, so it refuses a key neither takes.
        let unknown = read("targeting = \"none\"\nfastest = 1").unwrap_err();
        assert!(unknown.to_string().contains("fastest"), "{unknown}");
        // `train` has no section in the crate's readers.
        let unrun = read("targeting = \"none\"\nkind = \"train\"").unwrap_err();
        assert!(
            unrun.to_string().contains("no action of the kind `train`"),
            "{unrun}"
        );
        // The kind's numbers count its ranks.
        assert!(read_most.check_ranks(2) && !read_most.check_ranks(1));
    }

    #[test]
    fn the_pipeline_fields_are_the_keys_of_every_kind_but_the_name_the_targeting_and_the_when() {
        // The keys' fields but the name, the targeting and the `when`, every action's, and the
        // delivery and the lists, which read apart: one field each, in the keys' order.
        let mut keys: Vec<&str> = StructFields::of::<ActionKeys>()
            .iter()
            .copied()
            .filter(|&key| key != "name" && key != "targeting" && key != "when")
            .collect();
        keys.push(ActionData::DELIVERY);
        keys.extend(ActionData::LISTS);
        let mut fields = PipelineField::NAMES.to_vec();
        keys.sort_unstable();
        fields.sort_unstable();
        assert_eq!(fields, keys);
        // A kind takes a minimum range with any range, and a hold while ready with its passive.
        let takes = KindTakes::NONE.with(Takes::Must, &[PipelineField::Range]);
        assert_eq!(takes.of(PipelineField::MinRange), Takes::May);
        assert_eq!(takes.of(PipelineField::PassiveWhileReady), Takes::Never);
        assert!(takes.metered() && !KindTakes::ANY.metered());
        let passive = KindTakes::NONE.with(Takes::May, &[PipelineField::PassiveModifier]);
        assert_eq!(passive.of(PipelineField::PassiveWhileReady), Takes::May);
        assert_eq!(passive.of(PipelineField::MinRange), Takes::Never);
    }

    #[test]
    fn each_rank_holds_the_run_of_its_own_resource_costs() {
        // Costs in name order: gems 3 at both ranks, gold 50 then 75, and mana, a pool's, apart.
        let text = "targeting = \"none\"\ncost = { gold = [50, 75], gems = 3, mana = 10 }";
        let data = ActionData::read(Toml::parse(text).unwrap(), &TestKind::readers()).unwrap();
        let [gems, gold] = [0, 1].map(ResourceId::new);
        let target = |name: &DeclaredName| match name.as_str() {
            "gems" => Some(CostTarget::Resource(gems)),
            "gold" => Some(CostTarget::Resource(gold)),
            "mana" => PoolId::new(0).map(CostTarget::Pool),
            _ => None,
        };
        let rate = MatchRate::new(NonZeroU32::new(30).unwrap());
        let loaded = data.load_ranks(2, rate, target).unwrap();
        let cost = |resource, amount| ResourceAmount { resource, amount };
        let runs: Vec<&[ResourceAmount]> = (loaded.values.iter())
            .map(|values| &loaded.resource_costs[values.resource_cost.range()])
            .collect();
        assert_eq!(
            runs,
            [
                &[cost(gems, 3), cost(gold, 50)][..],
                &[cost(gems, 3), cost(gold, 75)][..],
            ]
        );
    }
}
