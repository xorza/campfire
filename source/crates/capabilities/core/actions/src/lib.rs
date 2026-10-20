// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::SystemSet;
use campfire_common::EnumAll;
use campfire_declarations::{
    ByType, Declarations, InstallCx, Module, ModuleSystems, NamedScripts, NamedTexts, SetPlace,
};
use campfire_sim::{Capability, SimSystems, Unpredicted};
use campfire_units::{BodyUnit, UnitsSystems};

use crate::actions_api::ActionsApi;
use crate::actions_column::RowReads;
use crate::holds::{Charges, Holds};

mod action;
mod action_book;
mod action_call;
mod action_data;
mod action_kind;
mod action_load;
mod action_names;
mod action_parts;
mod action_range;
mod action_ranks;
mod action_rules;
mod action_slots;
mod action_target;
mod actions_api;
mod actions_column;
mod actions_data;
mod actions_effect;
mod amount;
mod by_action;
mod cast_spends;
mod channel_call;
mod checked_action;
mod cost_target;
mod damage_kind_book;
mod delivery;
mod delivery_data;
mod dyn_eq;
mod effect_list_book;
mod effect_names;
mod error;
mod fan;
mod flights;
mod holds;
mod in_progress;
mod kind_section;
mod kind_takes;
mod launch_id;
mod list_hook;
mod listed_effect;
mod lists_of;
mod loaded_action;
mod loadout_ranks;
mod payer;
mod per_action;
mod pipeline_field;
mod planned_effect;
mod purse;
mod range_field;
mod rank_fields;
mod rank_values;
mod reached;
mod shot_damage;
mod shots;
mod slot_aim;
mod slot_claims;
mod slot_kind;
mod slot_kind_book;
mod slot_kind_data;
mod slot_kinds;
mod spawn_effect_data;
mod targeting;
mod targets;
mod track_names;
mod type_slots_data;

pub use crate::action::{Action, Aim, Passive};
pub use crate::action_book::{ActionBook, PackageAction};
pub use crate::action_call::{ActionCall, ResolvedCast, Started};
pub use crate::action_data::error::ActionField;
pub use crate::action_data::{ActionData, ChannelData, ChargeUpData, ChargesData, Toggle};
pub use crate::action_kind::ActionKind;
pub use crate::action_load::ActionLoad;
pub use crate::action_names::ActionNames;
pub use crate::action_parts::ActionParts;
pub use crate::action_range::ActionRange;
pub use crate::action_ranks::ActionRanks;
pub use crate::action_slots::action_progress::ActionProgress;
pub use crate::action_slots::action_progress::progress_dues::{
    AttackTargets, ChannelSteps, ProgressDues, ProgressIndexes, ResolveKind, Resolves,
};
pub use crate::action_slots::caster::Caster;
pub use crate::action_slots::slot_times::{
    ActionCheck, SlotCharges, SlotCommit, SlotTime, SlotTimes, StartChecked, StartFrom,
};
pub use crate::action_slots::timed_slots::TimedSlots;
pub use crate::action_slots::use_group::UseGroup;
pub use crate::action_slots::{ActionSlot, ActionSlots, SlotsMut, SlotsMutItem};
pub use crate::action_target::ActionTarget;
pub use crate::actions_column::{ActionsColumn, SlotRow};
pub use crate::actions_data::ActionsData;
pub use crate::amount::Amount;
pub use crate::by_action::ByAction;
pub use crate::cast_spends::CastSpends;
pub use crate::channel_call::{ChannelCall, ChannelStep};
pub use crate::checked_action::CheckedAction;
pub use crate::cost_target::CostTarget;
pub use crate::damage_kind_book::DamageKindBook;
pub use crate::delivery::{Delivery, DeliveryShape, ProjectileFlight};
pub use crate::delivery_data::DeliveryData;
pub use crate::dyn_eq::DynEq;
pub use crate::effect_list_book::{EffectListBook, EffectSpec, ListCall};
pub use crate::effect_names::EffectNames;
pub use crate::error::{ActionError, ActionProblem, SlotError, SlotKindsError};
pub use crate::fan::Fan;
pub use crate::flights::Flights;
pub use crate::in_progress::{InProgress, OrderPhase};
pub use crate::kind_section::{KindSection, KindSections};
pub use crate::kind_takes::{KindTakes, RangeTakes, Takes};
pub use crate::launch_id::LaunchId;
pub use crate::list_hook::{ListHook, OnEnd, OnHit, OnResolve};
pub use crate::listed_effect::effect_data::{EffectData, LaunchLists};
pub use crate::listed_effect::error::{EffectError, EffectRule};
pub use crate::listed_effect::listed_spec::{EffectTo, ListedAt, ListedSpec};
pub use crate::listed_effect::{
    EffectResolveCx, EffectRuleCx, ListedEffect, ListedEffects, RuleAt,
};
pub use crate::lists_of::{ListsOf, ReachHook};
pub use crate::loaded_action::LoadedAction;
pub use crate::loadout_ranks::LoadoutRanks;
pub use crate::payer::Payer;
pub use crate::per_action::PerAction;
pub use crate::pipeline_field::PipelineField;
pub use crate::planned_effect::PlannedEffect;
pub use crate::purse::Purse;
pub use crate::range_field::RangeField;
pub use crate::rank_fields::{RankChannel, RankCharges, RankFields, RankToggle, TogglePer};
pub use crate::rank_values::{ChannelRule, ChargeRule, LoadedRanks, RankValues};
pub use crate::reached::Reached;
pub use crate::shot_damage::ShotDamage;
pub use crate::shots::{Shot, Shots};
pub use crate::slot_aim::SlotAim;
pub use crate::slot_claims::{SlotClaim, SlotClaims};
pub use crate::slot_kind::SlotKind;
pub use crate::slot_kind_book::SlotKindBook;
pub use crate::slot_kind_data::{SlotKindData, SlotRanks};
pub use crate::slot_kinds::SlotKinds;
pub use crate::spawn_effect_data::SpawnEffectData;
pub use crate::targeting::Targeting;
pub use crate::targets::Targets;
pub use crate::track_names::TrackNames;
pub use crate::type_slots_data::TypeSlotsData;

/// The core's actions: every action a match loads, and the slots units hold them in.
#[derive(Debug)]
pub struct Actions;

/// The steps of the action pipeline, in each stage in the order listed, which each capability
/// that acts puts its own sets at: the steps' order is the order of every capability's part of
/// an action (02-engine-core.md, Sets order against the steps of the modules they build on).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum ActionsSystems {
    /// After the tick begins: toggled actions pay for the tick.
    Toggle,
    /// The tick's commands learn ranks.
    Learn,
    /// The tick's commands queue and cancel what units make.
    Queue,
    /// The tick's commands buy, sell and swap what units carry.
    Trade,
    /// The tick's commands order units.
    Order,
    /// Once the tick's orders and expiries are in: the actions' charges hold, before the
    /// grants of the passives that wait for a ready action.
    HoldAtInputs,
    /// Units chase their targets.
    Chase,
    /// Attacks wind up.
    WindUp,
    /// After attacks start: what each unit was ordered starts, each kind's orders by the
    /// capability that runs the kind.
    Start,
    /// The deliveries in flight travel.
    Travel,
    /// The tick's hits land.
    Hit,
    /// The deliveries due trigger.
    Trigger,
    /// Once the deliveries flew and triggered: their hits and ends run their actions' hooks.
    Deliver,
    /// The actions going off pay their toggles.
    Pay,
    /// The attacks going off strike, once the grants of the strike apply.
    Strike,
    /// Once the attacks going off struck: the tick's shots become launches.
    Fire,
    /// The due casts resolve.
    Cast,
    /// Once the due casts resolved: the tick's launches take off.
    Launch,
    /// The tick's areas land, before the modifiers' intervals come.
    Land,
    /// Once the tick's damage is dealt and before units die: what the damage launched lands,
    /// and the sources push what the tick spent.
    Resolve,
}

impl ModuleSystems for ActionsSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        let stage = SetPlace::stage;
        match self {
            Self::Toggle => stage(SimSystems::Inputs).after(UnitsSystems::Begin),
            Self::Learn => stage(SimSystems::Inputs).after(Self::Toggle),
            Self::Queue => stage(SimSystems::Inputs).after(Self::Learn),
            Self::Trade => stage(SimSystems::Inputs).after(Self::Queue),
            Self::Order => stage(SimSystems::Inputs).after(Self::Trade),
            Self::HoldAtInputs => stage(SimSystems::Inputs).after(Self::Order),
            Self::Chase => stage(SimSystems::Act),
            Self::WindUp => stage(SimSystems::Act).after(Self::Chase),
            Self::Start => stage(SimSystems::Act).after(Self::WindUp),
            Self::Travel => stage(SimSystems::Hit),
            Self::Hit => stage(SimSystems::Hit).after(Self::Travel),
            Self::Trigger => stage(SimSystems::Hit).after(Self::Hit),
            Self::Deliver => stage(SimSystems::Hit).after(Self::Trigger),
            Self::Pay => stage(SimSystems::Hit).after(Self::Deliver),
            Self::Strike => stage(SimSystems::Hit).after(Self::Pay),
            Self::Fire => stage(SimSystems::Hit).after(Self::Strike),
            Self::Cast => stage(SimSystems::Hit).after(Self::Fire),
            Self::Launch => stage(SimSystems::Hit).after(Self::Cast),
            Self::Land => stage(SimSystems::Hit).after(Self::Launch),
            Self::Resolve => stage(SimSystems::Resolve),
        }
    }
}

impl Module for Actions {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Systems = ActionsSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<ActionsData>();
        d.section::<TypeSlotsData>();
        d.section::<SlotKinds>();
        d.removals::<Unpredicted>();
        d.component::<ActionSlots>();
        d.component::<ActionProgress>();
        d.component::<SlotTimes>();
        d.timed::<Charges>();
        d.timed::<Resolves>();
        d.timed::<ChannelSteps>();
        d.timed::<AttackTargets>();
        d.scratch::<Shots>();
        d.require::<ActionSlots, SlotTimes>();
        d.require::<ActionSlots, ActionProgress>();
        d.fact::<ActionLoad>(ActionLoad::build);
        d.book::<ActionBook>(ActionLoad::book);
        d.book::<EffectListBook>(ActionLoad::effects);
        d.book::<SlotKindBook>(SlotKindBook::build);
        d.listed::<SpawnEffectData>();
        d.column::<ActionsColumn, RowReads>(ActionsColumn::fill_row);
        d.api(ActionsApi::register);
        d.book::<ByType<ActionSlots>>(ActionSlots::book);
        d.fact::<SlotClaims>(|_| Ok(SlotClaims::default()));
        d.rule(SlotClaims::rule);
        d.rule(Self::rule);
        d.fact::<ActionRanks>(ActionRanks::build);
        d.rule(ActionRanks::rule);
        d.extend::<NamedScripts>(Self::named_scripts);
        d.extend::<NamedTexts>(Self::named_texts);
        d.part::<BodyUnit, ActionSlots>(|at| at.of_type::<ActionSlots>().cloned());
        d.part::<BodyUnit, SlotTimes>(|at| at.of_type::<ActionSlots>().map(SlotTimes::ready));
        d.part::<BodyUnit, ActionProgress>(|at| {
            at.of_type::<ActionSlots>()
                .map(|_| ActionProgress::default())
        });
    }

    /// Adds the actions to a match whose core is installed, with none loaded yet, and the holds
    /// of every action's charges as each tick starts, which a cast of any match draws. The start
    /// of an action is the abilities'.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.add(ActionsSystems::HoldAtInputs, Holds::hold_charges);
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::action_slots::action_progress::progress_dues::internals::LooseDues;

    use std::num::NonZeroU8;

    use bevy_ecs::world::World;
    use campfire_common::{DeclaredName, Ticks};
    use campfire_declarations::Book;
    use campfire_math::Num;
    use campfire_players::ResourceAmount;
    use campfire_script::{HookSet, MatchNames, UnknownName};
    use campfire_units::{PoolCost, PoolId};
    use campfire_values::{
        ActionId, DamageKind, ModifierId, Named, PackageIndex, Param, Tag, TrackId, TypeScope,
        UnitType,
    };

    use crate::Actions;
    use crate::action::{Action, Aim};
    use crate::action_book::ActionBook;
    use crate::action_data::ActionData;
    use crate::action_kind::ActionKind;
    use crate::action_range::ActionRange;
    use crate::delivery::{Delivery, DeliveryShape, ProjectileFlight};
    use crate::effect_list_book::EffectListBook;
    use crate::effect_names::EffectNames;
    use crate::fan::Fan;
    use crate::rank_fields::{RankToggle, TogglePer};
    use crate::rank_values::{CostRun, RankValues};

    impl Actions {
        /// The action `name` of `package`, as the match loaded it.
        pub fn named(world: &World, package: PackageIndex, name: &str) -> Option<ActionId> {
            world
                .resource::<ActionBook>()
                .names()
                .named((package, name))
        }

        /// Adds `action`, named `name` in its package, of no script, passive or hold, of one
        /// rank of no cooldown, charges, toggle, channel or charge, to the match's action book.
        pub fn add(world: &mut World, name: &str, action: TestAction) -> ActionId {
            let resource_costs: Box<[ResourceAmount]> = action.resource_cost.into_iter().collect();
            Self::push(
                world,
                name,
                Action {
                    package: PackageIndex::MODE,
                    kind: action.kind,
                    passive: None,
                    hold: None,
                    aim: action.aim,
                    ranks: vec![RankValues {
                        range: action.range,
                        cooldown: Ticks::ZERO,
                        cost: action.cost,
                        windup: action.windup,
                        charges: None,
                        toggle: None,
                        channel: None,
                        charge: None,
                        resource_cost: CostRun::whole(&resource_costs),
                    }],
                    resource_costs,
                    script: None,
                    hooks: HookSet::default(),
                    delivery: action.delivery,
                },
            )
        }

        /// Adds a toggle that pays `cost` as each attack of its caster goes off, at no other
        /// cost, to the match's action book.
        pub fn add_attack_toggle(world: &mut World, cost: PoolCost) -> ActionId {
            Self::push(
                world,
                "toggle",
                Action {
                    package: PackageIndex::MODE,
                    kind: ActionKind::Cast,
                    passive: None,
                    hold: None,
                    aim: Aim::None,
                    ranks: vec![RankValues {
                        range: ActionRange::meters(Num::ZERO),
                        cooldown: Ticks::ZERO,
                        cost: PoolCost::default(),
                        windup: Ticks::ZERO,
                        charges: None,
                        toggle: Some(RankToggle {
                            per: TogglePer::Attack,
                            cost,
                        }),
                        channel: None,
                        charge: None,
                        resource_cost: CostRun::EMPTY,
                    }],
                    resource_costs: Box::default(),
                    script: None,
                    hooks: HookSet::default(),
                    delivery: None,
                },
            )
        }

        /// Adds `action`, named `name` in its package, to the match's action book, and shares
        /// the book with the script view.
        pub fn push(world: &mut World, name: &str, action: Action) -> ActionId {
            let mut book = world.resource_mut::<ActionBook>();
            let id = book.push(name, action);
            ActionBook::share_in(world);
            id
        }
    }

    /// An action tests add: its kind, its aim, its range and its windup at its one rank, its cost
    /// in pools and in a player resource, and its delivery.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TestAction {
        pub kind: ActionKind,
        pub aim: Aim,
        pub range: ActionRange,
        pub windup: Ticks,
        pub cost: PoolCost,
        pub resource_cost: Option<ResourceAmount>,
        pub delivery: Option<Delivery>,
    }

    impl TestAction {
        /// An action of `kind` at `aim` within `range` that winds up `windup`, costing nothing
        /// and delivering at once.
        pub fn new(kind: ActionKind, aim: Aim, range: ActionRange, windup: Ticks) -> Self {
            Self {
                kind,
                aim,
                range,
                windup,
                cost: PoolCost::default(),
                resource_cost: None,
                delivery: None,
            }
        }

        /// The same action, delivering a homing projectile of `unit_type`.
        #[must_use]
        pub const fn homing(self, unit_type: UnitType) -> Self {
            Self {
                delivery: Some(Delivery {
                    unit_type,
                    shape: DeliveryShape::Projectile {
                        fan: Fan {
                            count: NonZeroU8::MIN,
                            spread_deg: Num::ZERO,
                        },
                        flight: ProjectileFlight::Homing,
                    },
                }),
                ..self
            }
        }
    }

    /// The names of an action's effect lists as a match's world resolves them: its names, the
    /// action's params as its data names them, and the modifiers of the action's package.
    #[derive(Debug)]
    pub struct MatchEffectNames<'w> {
        world: &'w World,
        data: &'w ActionData,
        package: PackageIndex,
    }

    impl Actions {
        /// Loads the effect lists of `action` of `package`, which loaded last from `data`, which
        /// the package load checked: each name resolved to its id, each param to its place among
        /// the action's params.
        pub fn load_effect_lists(
            world: &mut World,
            action: ActionId,
            package: PackageIndex,
            data: &ActionData,
        ) {
            let mut lists = world.remove_resource::<EffectListBook>().unwrap();
            let names = MatchEffectNames::new(world, data, package);
            lists
                .push(action, data, &names)
                .expect("a test's effect lists name what its match has");
            world.insert_resource(lists);
        }
    }

    impl<'w> MatchEffectNames<'w> {
        pub const fn new(world: &'w World, data: &'w ActionData, package: PackageIndex) -> Self {
            MatchEffectNames {
                world,
                data,
                package,
            }
        }
    }

    impl EffectNames for MatchEffectNames<'_> {
        fn param(&self, name: &DeclaredName) -> Result<usize, UnknownName> {
            self.data
                .params
                .keys()
                .position(|held| held == name)
                .ok_or_else(|| UnknownName::new(Param::NAMES, name))
        }

        fn damage_kind(&self, name: &DeclaredName) -> Result<DamageKind, UnknownName> {
            self.world
                .resource::<MatchNames>()
                .named(name.as_str())
                .ok_or_else(|| UnknownName::new(DamageKind::NAMES, name))
        }

        fn pool(&self, name: &DeclaredName) -> Result<PoolId, UnknownName> {
            let pools = self.world.resource::<MatchNames>().table::<PoolId>();
            pools
                .and_then(|pools| pools.named(name.as_str()))
                .ok_or_else(|| UnknownName::new(PoolId::NAMES, name))
        }

        fn modifier(&self, name: &DeclaredName) -> Result<ModifierId, UnknownName> {
            let modifiers = self.world.resource::<MatchNames>().table::<ModifierId>();
            modifiers
                .and_then(|modifiers| modifiers.named((self.package, name.as_str())))
                .ok_or_else(|| UnknownName::new(ModifierId::NAMES, name))
        }

        fn track(&self, name: &DeclaredName) -> Result<TrackId, UnknownName> {
            self.world
                .resource::<MatchNames>()
                .named(name.as_str())
                .ok_or_else(|| UnknownName::new(TrackId::NAMES, name))
        }

        fn tag(&self, name: &DeclaredName) -> Result<Tag, UnknownName> {
            self.world
                .resource::<MatchNames>()
                .tags()
                .named(name.as_str())
                .ok_or_else(|| UnknownName::new(Tag::NAMES, name))
        }

        fn package_type(&self, name: &DeclaredName) -> Result<UnitType, UnknownName> {
            let scope = TypeScope::of_package(self.package);
            self.world
                .resource::<MatchNames>()
                .named((scope, name.as_str()))
                .ok_or_else(|| UnknownName::new(UnitType::NAMES, name))
        }
    }
}
