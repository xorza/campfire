use std::any::Any;
use std::collections::BTreeMap;
use std::num::NonZeroU8;
use std::slice;
use std::sync::Arc;

use campfire_codec::{StructFields, TomlError, TomlTable, TomlValue};
use campfire_common::DeclaredName;
use campfire_declarations::SectionReaders;
use campfire_math::Num;
use campfire_players::ResourceAmount;
use campfire_script::DataTable;
use campfire_units::PoolCost;
use campfire_values::{
    Distance, FilterData, Number, PackagePath, Param, Rank, Ranked, Scalar, TextId,
};
use serde::de::Error;
use serde::{Deserialize, Serialize};

use crate::action_data::error::ActionField;
use crate::action_kind::ActionKind;
use crate::action_range::ActionRange;
use crate::cost_target::CostTarget;
use crate::delivery_data::DeliveryData;
use crate::kind_section::{KindSection, KindSections};
use crate::list_hook::ListHook;
use crate::listed_effect::effect_data::EffectData;
use crate::listed_effect::error::EffectRule;
use crate::range_field::RangeField;
use crate::rank_fields::{RankChannel, RankCharges, RankFields, RankToggle, TogglePer};
use crate::targeting::Targeting;

pub(crate) mod error;

/// An action as its package's `[actions.<id>]` declares it, in milliseconds: the pipeline's
/// fields, each capability field one value or one per rank, and its kind's section, which reads
/// the rest of its table (04-capabilities/actions.md, Each kind owns its section).
#[derive(Debug, Clone)]
pub struct ActionData {
    /// The message of the name players see, which an avatar's or a loadout's action needs.
    pub name: Option<TextId>,
    /// The script, if the action needs one.
    pub script: Option<PackagePath>,
    pub targeting: Targeting,
    pub range: Option<Ranked<RangeField>>,
    /// The meters nearer than which it takes no target (12-missiles.md, A minimum range).
    pub min_range: Option<Ranked<Number>>,
    pub cooldown_ms: Option<Ranked<Number>>,
    /// In each pool of the caster it names, and each resource of the caster's player.
    pub cost: BTreeMap<DeclaredName, Ranked<Number>>,
    pub windup_ms: Option<Ranked<Number>>,
    /// A target beyond range is moved in, instead of the caster walking.
    pub clamp_to_range: bool,
    pub toggle: Option<Toggle>,
    pub channel: Option<ChannelData>,
    /// The modifier held while the toggle is on or the channel runs.
    pub hold: Option<DeclaredName>,
    pub charges: Option<ChargesData>,
    /// A charged cast.
    pub charging: Option<ChargeUpData>,
    /// The modifier held while the action has a rank.
    pub passive_modifier: Option<DeclaredName>,
    /// The passive modifier is held only while the action is off cooldown.
    pub passive_while_ready: bool,
    /// How it reaches what it affects, other than at once.
    pub delivery: Option<DeliveryData>,
    /// Values for the script, as `ctx.params` reads them.
    pub params: BTreeMap<DeclaredName, Param>,
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
            name,
            script,
            targeting,
            range,
            min_range,
            cooldown_ms,
            cost,
            windup_ms,
            clamp_to_range,
            toggle,
            channel,
            hold,
            charges,
            charging,
            passive_modifier,
            passive_while_ready,
            delivery,
            params,
            section,
            on_resolve,
            on_hit,
            on_end,
        } = self;
        *name == other.name
            && *script == other.script
            && *targeting == other.targeting
            && *range == other.range
            && *min_range == other.min_range
            && *cooldown_ms == other.cooldown_ms
            && *cost == other.cost
            && *windup_ms == other.windup_ms
            && *clamp_to_range == other.clamp_to_range
            && *toggle == other.toggle
            && *channel == other.channel
            && *hold == other.hold
            && *charges == other.charges
            && *charging == other.charging
            && *passive_modifier == other.passive_modifier
            && *passive_while_ready == other.passive_while_ready
            && *delivery == other.delivery
            && *params == other.params
            && **section == *other.section
            && *on_resolve == other.on_resolve
            && *on_hit == other.on_hit
            && *on_end == other.on_end
    }
}

impl Eq for ActionData {}

/// The keys of an action's table that the pipeline reads, every kind's: its kind's name, which
/// picks the reader of its section, and the fields of `ActionData` but its section and its lists,
/// which read with the release's readers. A tool writes an action through them, beside its
/// kind's section (03-game-scripting.md, A section has a partial form).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionKeys {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<TextId>,
    #[serde(default)]
    pub kind: ActionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<PackagePath>,
    pub targeting: Targeting,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Ranked<RangeField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_range: Option<Ranked<Number>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_ms: Option<Ranked<Number>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cost: BTreeMap<DeclaredName, Ranked<Number>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windup_ms: Option<Ranked<Number>>,
    #[serde(default, skip_serializing_if = "is_off")]
    pub clamp_to_range: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggle: Option<Toggle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<ChannelData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<DeclaredName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charges: Option<ChargesData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charging: Option<ChargeUpData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passive_modifier: Option<DeclaredName>,
    #[serde(default, skip_serializing_if = "is_off")]
    pub passive_while_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryData>,
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
    /// The keys of an action of `kind` that targets as `targeting`, and gives no other key.
    pub const fn new(kind: ActionKind, targeting: Targeting) -> Self {
        Self {
            name: None,
            kind,
            script: None,
            targeting,
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
            delivery: None,
            params: BTreeMap::new(),
        }
    }

    /// The table of the action of these keys and `section`, the fields of its kind's own, side
    /// by side, as `ActionData::read` takes them apart.
    pub fn table<K: KindSection + Serialize>(&self, section: &K) -> TomlTable {
        debug_assert_eq!(
            self.kind,
            section.kind(),
            "an action's section is of its kind"
        );
        let mut table: TomlTable = TomlValue::of(self).read().expect("the keys write a table");
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

/// A channel, which starts after `on_resolve` and calls `on_channel_tick` every `tick_ms`; with
/// `damage_interrupts`, damage of more than 0 to its unit cuts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelData {
    pub duration_ms: Ranked<Number>,
    pub tick_ms: Ranked<Number>,
    #[serde(default)]
    pub damage_interrupts: bool,
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
        let range = self.range.as_ref().and_then(Ranked::ranks);
        let numbers = self.ranked_numbers().chain(self.costs());
        let numbers = numbers.chain(self.section.numbers()).map(Ranked::ranks);
        let params = self.params.values().map(Param::ranks);
        [range].into_iter().chain(numbers).chain(params).flatten()
    }

    /// Each of the pipeline's fields it gives that is a number at each rank, which may read a
    /// param: its minimum range, cooldown, windup, channel, charges and charge.
    fn ranked_numbers(&self) -> impl Iterator<Item = &Ranked<Number>> {
        [
            self.min_range.as_ref(),
            self.cooldown_ms.as_ref(),
            self.windup_ms.as_ref(),
            self.channel.as_ref().map(|channel| &channel.duration_ms),
            self.channel.as_ref().map(|channel| &channel.tick_ms),
            self.charges.as_ref().map(|charges| &charges.max),
            self.charges.as_ref().map(|charges| &charges.recharge_ms),
            self.charging.as_ref().map(|charge| &charge.max_ms),
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
        let whole = |field, ranked: Option<&Ranked<Number>>| self.whole(rank, field, ranked);
        let range = match &self.range {
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
        let min_range = self.min_range.as_ref().map(|ranked| {
            let meters = self.value_at(rank, ranked).and_then(Scalar::to_num);
            meters.and_then(Distance::new).ok_or(ActionField::MinRange)
        });
        Ok(RankFields {
            range,
            min_range: min_range.transpose()?,
            cooldown_ms: whole(ActionField::Cooldown, self.cooldown_ms.as_ref())?,
            cost: PoolCost::new(cost),
            resource_cost,
            windup_ms: whole(ActionField::Windup, self.windup_ms.as_ref())?,
            charges: self.charges_at(rank)?,
            toggle: self.toggle_at(rank, &target)?,
            channel: self.channel_at(rank)?,
            charge_ms: self.charge_at(rank)?,
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
        match self.params.get(name) {
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
    fn charges_at(&self, rank: Rank) -> Result<Option<RankCharges>, ActionField> {
        let field = ActionField::Charges;
        self.charges
            .as_ref()
            .map(|charges| {
                let max = self.whole(rank, field, Some(&charges.max))?;
                let max = u8::try_from(max).ok().and_then(NonZeroU8::new);
                Ok(RankCharges {
                    max: max.ok_or(field)?,
                    recharge_ms: self.whole(rank, field, Some(&charges.recharge_ms))?,
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
    fn charge_at(&self, rank: Rank) -> Result<Option<u64>, ActionField> {
        let field = ActionField::Charge;
        self.charging
            .as_ref()
            .map(|charge| {
                let most = self.whole(rank, field, Some(&charge.max_ms))?;
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
                let duration_ms = self.whole(rank, field, Some(&channel.duration_ms))?;
                let tick_ms = self.whole(rank, field, Some(&channel.tick_ms))?;
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
            .chain(self.section.numbers())
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

    /// The keys of its effect lists.
    pub const LISTS: [&'static str; 3] = ["on_resolve", "on_hit", "on_end"];

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
        keys.chain(Self::LISTS)
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
        let keys = table.take_keys(StructFields::of::<ActionKeys>().iter().copied());
        let ActionKeys {
            name,
            kind,
            script,
            targeting,
            range,
            min_range,
            cooldown_ms,
            cost,
            windup_ms,
            clamp_to_range,
            toggle,
            channel,
            hold,
            charges,
            charging,
            passive_modifier,
            passive_while_ready,
            delivery,
            params,
        } = keys.read()?;
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
            name,
            script,
            targeting,
            range,
            min_range,
            cooldown_ms,
            cost,
            windup_ms,
            clamp_to_range,
            toggle,
            channel,
            hold,
            charges,
            charging,
            passive_modifier,
            passive_while_ready,
            delivery,
            params,
            section,
            on_resolve: on_resolve?,
            on_hit: on_hit?,
            on_end: on_end?,
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
        match &self.targeting {
            Targeting::Unit(filter) => Some(filter),
            Targeting::None | Targeting::Point | Targeting::Direction => None,
        }
        .into_iter()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use crate::action_data::ActionData;
    use crate::kind_section::KindSection;
    use crate::targeting::Targeting;

    impl ActionData {
        /// An action of `section`'s kind, of `targeting` and nothing more, as a table that names
        /// its kind and its targeting alone reads.
        pub fn of(targeting: Targeting, section: impl KindSection) -> Self {
            Self {
                name: None,
                script: None,
                targeting,
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
                delivery: None,
                params: BTreeMap::new(),
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
            read_most.cooldown_ms,
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
        // `enter` has no section in the crate's readers.
        let unrun = read("targeting = \"none\"\nkind = \"enter\"").unwrap_err();
        assert!(
            unrun.to_string().contains("no action of the kind `enter`"),
            "{unrun}"
        );
        // The kind's numbers count its ranks.
        assert!(read_most.check_ranks(2) && !read_most.check_ranks(1));
    }
}
