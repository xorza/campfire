#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use campfire_declare::{ByType, Declarations, InstallCx, Module, ModuleSet, Phase, SetPlace};
use campfire_sim::Capability;
use campfire_stats::ParamBook;
use campfire_units::BodyUnit;

use crate::actions_api::ActionsApi;
use crate::actions_column::RowReads;
use crate::holds::Holds;

mod action;
mod action_book;
mod action_call;
mod action_data;
mod action_data_field;
mod action_kind;
mod action_names;
mod action_parts;
mod action_range;
mod action_slots;
mod action_target;
mod actions_api;
mod actions_column;
mod actions_effect;
mod amount;
mod capability_does_spec;
mod cast_spends;
mod channel_call;
mod checked_action;
mod construct_data;
mod cost_target;
mod delivery;
mod delivery_data;
mod does_spec;
mod effect_data;
mod effect_list_book;
mod effect_names;
mod effect_queues;
mod error;
mod fan;
mod gather_spec;
mod holds;
mod in_progress;
mod kind_data;
mod kind_spec;
mod launch_id;
mod lists_of;
mod payer;
mod placement_data;
mod purse;
mod range_field;
mod rank_fields;
mod rank_values;
mod ready_waits;
mod requires_data;
mod shot_hits;
mod shots;
mod slot_aim;
mod slot_kind;
mod slot_kind_book;
mod slot_kind_data;
mod slot_kinds;
mod targeting;
mod targets;
mod use_group;
mod weapon;

pub use crate::action::{Action, Aim, Passive};
pub use crate::action_book::ActionBook;
pub use crate::action_call::{ActionCall, ResolvedCast, Started};
pub use crate::action_data::error::{ActionField, EffectRule};
pub use crate::action_data::{ActionData, ChannelData, ChargeUpData, ChargesData, Toggle};
pub use crate::action_data_field::ActionDataField;
pub use crate::action_kind::ActionKind;
pub use crate::action_names::ActionNames;
pub use crate::action_parts::ActionParts;
pub use crate::action_range::ActionRange;
pub use crate::action_slots::slot_times::{
    ActionCheck, SlotCharges, SlotTime, SlotTimes, TimedSlots,
};
pub use crate::action_slots::{ActionSlot, ActionSlots};
pub use crate::action_target::ActionTarget;
pub use crate::actions_column::{ActionsColumn, SlotRow};
pub use crate::amount::Amount;
pub use crate::capability_does_spec::CapabilityDoesSpec;
pub use crate::cast_spends::CastSpends;
pub use crate::channel_call::{ChannelCall, ChannelStep};
pub use crate::checked_action::CheckedAction;
pub use crate::construct_data::ConstructData;
pub use crate::cost_target::CostTarget;
pub use crate::delivery::{Delivery, DeliveryShape};
pub use crate::delivery_data::DeliveryData;
pub use crate::effect_data::{
    DamageFields, DoesData, EffectData, EffectTo, HealFields, LaunchFields, ModifierFields,
    MoveData, PlannedEffect, PurgeFields, RestoreFields, SpawnFields, XpFields,
};
pub use crate::effect_list_book::{EffectListBook, EffectSpec};
pub use crate::effect_names::EffectNames;
pub use crate::effect_queues::{EffectQueues, QueueListed};
pub use crate::error::{ActionError, ActionProblem};
pub use crate::fan::Fan;
pub use crate::gather_spec::GatherSpec;
pub use crate::in_progress::{InProgress, OrderPhase};
pub use crate::kind_data::KindData;
pub use crate::kind_spec::KindSpec;
pub use crate::launch_id::LaunchId;
pub use crate::lists_of::ListsOf;
pub use crate::payer::Payer;
pub use crate::placement_data::{PlacementData, PlacementRule};
pub use crate::purse::Purse;
pub use crate::range_field::RangeField;
pub use crate::rank_fields::{RankChannel, RankCharges, RankFields, RankToggle, TogglePer};
pub use crate::rank_values::{ChannelRule, ChargeRule, LoadedRanks, RankValues};
pub use crate::requires_data::RequiresData;
pub use crate::shot_hits::{ShotHit, ShotHits};
pub use crate::shots::{Shot, Shots};
pub use crate::slot_aim::SlotAim;
pub use crate::slot_kind::SlotKind;
pub use crate::slot_kind_book::SlotKindBook;
pub use crate::slot_kind_data::{SlotKindData, SlotRanks};
pub use crate::slot_kinds::SlotKinds;
pub use crate::targeting::Targeting;
pub use crate::targets::Targets;
pub use crate::use_group::UseGroup;
pub use crate::weapon::Weapon;

/// The core's actions: every action a match loads, and the slots units hold them in.
#[derive(Debug)]
pub struct Actions;

campfire_common::enum_all! {
    /// The steps of the action pipeline. Each capability that acts puts its own steps in the
    /// phases between them.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum ActionsSet {
        /// After attacks start: what each unit was ordered starts, each kind's orders by the
        /// capability that runs the kind.
        Start,
        /// Once the deliveries flew and triggered: their hits and ends run their actions' hooks.
        Deliver,
        /// Once the attacks going off struck: the tick's shots become launches.
        Fire,
        /// Once the due casts resolved: the tick's launches take off.
        Launch,
        /// The tick's areas land.
        Land,
        /// Once the tick's damage is dealt and before units die: what the damage launched lands,
        /// and the sources push what the tick spent.
        Resolve,
        /// Once the tick's orders and expiries are in: passives and holds push their grants.
        HoldAtInputs,
        /// Once the attacks going off paid their toggles and before they strike: holds push the
        /// grants of the toggles an unpaid cost turned off.
        HoldAtStrike,
        /// Once the tick's damage is dealt and before units die: passives and holds push their
        /// grants.
        HoldAtResolve,
        /// Passives and holds push the grants of what the tick changed.
        HoldAtEnd,
    }
    /// Every step.
    pub const ALL;
}

impl ModuleSet for ActionsSet {
    const CAPABILITY: Option<Capability> = None;
    const ALL: &'static [ActionsSet] = &ActionsSet::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            ActionsSet::Start => Phase::Start,
            ActionsSet::Deliver => Phase::Deliver,
            ActionsSet::Fire => Phase::Fire,
            ActionsSet::Launch => Phase::Launch,
            ActionsSet::Land => Phase::Land,
            ActionsSet::Resolve => Phase::Resolve,
            ActionsSet::HoldAtInputs => Phase::HoldAtInputs,
            ActionsSet::HoldAtStrike => Phase::HoldAtStrike,
            ActionsSet::HoldAtResolve => Phase::HoldAtResolve,
            ActionsSet::HoldAtEnd => Phase::HoldAtEnd,
        })
    }
}

impl Module for Actions {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<ActionsSet>();
        d.component::<ActionSlots>();
        d.component::<SlotTimes>();
        d.scratch::<Shots>();
        d.scratch::<ShotHits>();
        d.require::<ActionSlots, SlotTimes>();
        d.book::<ActionBook>();
        d.book::<ParamBook>();
        d.book::<EffectListBook>();
        d.book::<SlotKindBook>();
        d.calls::<EffectQueues>();
        d.column::<ActionsColumn, RowReads>(ActionsColumn::fill_row);
        d.api(ActionsApi::register);
        d.book::<ByType<ActionSlots>>();
        d.part::<BodyUnit, ActionSlots>(|at| at.of_type::<ActionSlots>().cloned());
        d.part::<BodyUnit, SlotTimes>(|at| at.of_type::<ActionSlots>().map(SlotTimes::ready));
    }

    /// Adds the actions to a match whose core is installed, with none loaded yet, and, in a match
    /// with combat, the holds of every action: its charges as each tick starts, and its passives
    /// then and as each stage that changes ranks or cooldowns ends. A match with no combat holds
    /// none, as every action's unit target is one combat finds; the start of an action is the
    /// abilities'.
    fn install(cx: &mut InstallCx<'_>) {
        if cx.has(Capability::Combat) {
            Actions::schedule(cx.schedule);
        }
    }
}

impl Actions {
    fn schedule(schedule: &mut Schedule) {
        schedule.add_systems((
            (Holds::hold_charges, Holds::want_passives)
                .chain()
                .in_set(ActionsSet::HoldAtInputs),
            Holds::want_passives.in_set(ActionsSet::HoldAtStrike),
            Holds::want_passives.in_set(ActionsSet::HoldAtResolve),
            Holds::want_passives.in_set(ActionsSet::HoldAtEnd),
        ));
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use std::num::NonZeroU8;

    use bevy_ecs::world::World;
    use campfire_common::Ticks;
    use campfire_declare::Book;
    use campfire_math::Num;
    use campfire_players::ResourceAmount;
    use campfire_script::{Filter, HookSet, NameKind, UnknownName, View};
    use campfire_stats::{ParamBook, PoolCost, PoolId, StatId, Stats};
    use campfire_values::{
        ActionId, DamageKind, DeclaredName, ModifierId, Tag, TrackId, TypeScope, UnitType,
    };

    use crate::Actions;
    use crate::action::{Action, Aim};
    use crate::action_book::ActionBook;
    use crate::action_data::ActionData;
    use crate::action_range::ActionRange;
    use crate::delivery::{Delivery, DeliveryShape};
    use crate::effect_list_book::EffectListBook;
    use crate::effect_names::EffectNames;
    use crate::fan::Fan;
    use crate::gather_spec::GatherSpec;
    use crate::kind_spec::KindSpec;
    use crate::rank_values::RankValues;
    use crate::weapon::Weapon;

    impl Actions {
        /// The action `name` of `package`, as the match loaded it.
        pub fn named(world: &World, package: u16, name: &str) -> Option<ActionId> {
            world
                .resource::<ActionBook>()
                .names()
                .named((package, name))
        }

        /// Adds a gather of `spec` from the nodes `aim` selects, within `range` meters, a trip of
        /// `time`, to the match's action book.
        pub fn add_gather(
            world: &mut World,
            spec: GatherSpec,
            aim: Filter,
            range: Num,
            time: Ticks,
        ) -> ActionId {
            Actions::push(
                world,
                "gather",
                Action {
                    package: 0,
                    kind: KindSpec::Gather(spec),
                    passive: None,
                    hold: None,
                    aim: Aim::Unit(aim),
                    ranks: vec![RankValues {
                        range: ActionRange::Meters(range),
                        cooldown: Ticks::ZERO,
                        cost: PoolCost::default(),
                        windup: time,
                        charges: None,
                        toggle: None,
                        channel: None,
                        charge: None,
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

    /// A weapon tests arm units with: what it aims at, its range, its windup, the type of the
    /// homing projectile it fires, if it fires one, the places among its unit's stats of its
    /// rate and its damage, and its cost in pools and in a player resource.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TestWeapon {
        pub aim: Filter,
        pub range: ActionRange,
        pub windup: Ticks,
        pub projectile: Option<UnitType>,
        pub rate: StatId,
        pub damage: StatId,
        pub cost: PoolCost,
        pub resource_cost: Option<ResourceAmount>,
    }

    /// The names of an action's effect lists as a match's world resolves them: its view, its param
    /// book, and the modifiers of the action's package.
    #[derive(Debug)]
    pub struct MatchEffectNames<'w> {
        world: &'w World,
        view: &'w View,
        action: ActionId,
        package: u16,
    }

    impl TestWeapon {
        /// A melee weapon at `aim` within `range` that winds up `windup`, of the first two stats'
        /// rate and damage, costing nothing.
        pub fn new(aim: Filter, range: ActionRange, windup: Ticks) -> TestWeapon {
            TestWeapon {
                aim,
                range,
                windup,
                projectile: None,
                rate: StatId::new(0),
                damage: StatId::new(1),
                cost: PoolCost::default(),
                resource_cost: None,
            }
        }
    }

    impl Actions {
        /// Adds `weapon` to the match's action book, which deals damage of the first kind.
        pub fn add_weapon(world: &mut World, weapon: TestWeapon) -> ActionId {
            let delivery = weapon.projectile.map(|unit_type| Delivery {
                unit_type,
                shape: DeliveryShape::Projectile {
                    fan: Fan {
                        count: NonZeroU8::MIN,
                        spread_deg: Num::ZERO,
                    },
                    homes: true,
                },
            });
            Actions::push(
                world,
                "weapon",
                Action {
                    package: 0,
                    kind: KindSpec::Attack(Weapon {
                        rate: weapon.rate,
                        damage: weapon.damage,
                        kind: DamageKind::new(0),
                    }),
                    passive: None,
                    hold: None,
                    aim: Aim::Unit(weapon.aim),
                    ranks: vec![RankValues {
                        range: weapon.range,
                        cooldown: Ticks::ZERO,
                        cost: weapon.cost,
                        windup: weapon.windup,
                        charges: None,
                        toggle: None,
                        channel: None,
                        charge: None,
                    }],
                    resource_costs: weapon.resource_cost.into_iter().collect(),
                    script: None,
                    hooks: HookSet::default(),
                    delivery,
                },
            )
        }

        /// Loads the effect lists of `action` of `package`, which loaded last from `data`, which
        /// the package load checked: each name resolved to its id, each param to its place among
        /// the action's params.
        pub fn load_effect_lists(
            world: &mut World,
            action: ActionId,
            package: u16,
            data: &ActionData,
        ) {
            let view = world.non_send::<View>().clone();
            let mut lists = world.remove_resource::<EffectListBook>().unwrap();
            let names = MatchEffectNames::new(world, &view, action, package);
            lists
                .push(action, data, &names)
                .expect("a test's effect lists name what its match has");
            world.insert_resource(lists);
        }

        /// Adds a train of `unit_type` that takes `time` and costs `resource_cost`, no pool and no
        /// cooldown, to the match's action book.
        pub fn add_train(
            world: &mut World,
            unit_type: UnitType,
            time: Ticks,
            resource_cost: Option<ResourceAmount>,
        ) -> ActionId {
            Actions::push(
                world,
                "train",
                Action {
                    package: 0,
                    kind: KindSpec::Train(unit_type),
                    passive: None,
                    hold: None,
                    aim: Aim::None,
                    ranks: vec![RankValues {
                        range: ActionRange::Global,
                        cooldown: Ticks::ZERO,
                        cost: PoolCost::default(),
                        windup: time,
                        charges: None,
                        toggle: None,
                        channel: None,
                        charge: None,
                    }],
                    resource_costs: resource_cost.into_iter().collect(),
                    script: None,
                    hooks: HookSet::default(),
                    delivery: None,
                },
            )
        }

        /// Adds a build of `unit_type` within `range` meters, aimed at a point, that takes `time` and costs
        /// `resource_cost`, no pool and no cooldown, to the match's action book.
        pub fn add_build(
            world: &mut World,
            unit_type: UnitType,
            range: impl Into<Num>,
            time: Ticks,
            resource_cost: Option<ResourceAmount>,
        ) -> ActionId {
            let range = range.into();
            Actions::push(
                world,
                "build",
                Action {
                    package: 0,
                    kind: KindSpec::Build(unit_type),
                    passive: None,
                    hold: None,
                    aim: Aim::Point { clamp: false },
                    ranks: vec![RankValues {
                        range: ActionRange::Meters(range),
                        cooldown: Ticks::ZERO,
                        cost: PoolCost::default(),
                        windup: time,
                        charges: None,
                        toggle: None,
                        channel: None,
                        charge: None,
                    }],
                    resource_costs: resource_cost.into_iter().collect(),
                    script: None,
                    hooks: HookSet::default(),
                    delivery: None,
                },
            )
        }
    }

    impl<'w> MatchEffectNames<'w> {
        pub const fn new(world: &'w World, view: &'w View, action: ActionId, package: u16) -> Self {
            MatchEffectNames {
                world,
                view,
                action,
                package,
            }
        }
    }

    impl EffectNames for MatchEffectNames<'_> {
        fn param(&self, name: &DeclaredName) -> Result<usize, UnknownName> {
            self.world
                .resource::<ParamBook>()
                .actions()
                .named(self.action.index(), name.as_str())
                .ok_or_else(|| UnknownName::new(NameKind::Param, name))
        }

        fn damage_kind(&self, name: &DeclaredName) -> Result<DamageKind, UnknownName> {
            self.view
                .names()
                .named(name.as_str())
                .ok_or_else(|| UnknownName::new(NameKind::DamageKind, name))
        }

        fn pool(&self, name: &DeclaredName) -> Result<PoolId, UnknownName> {
            let names = self.view.names();
            let pools = names.table::<PoolId>();
            pools
                .and_then(|pools| pools.named(name.as_str()))
                .ok_or_else(|| UnknownName::new(NameKind::Pool, name))
        }

        fn modifier(&self, name: &DeclaredName) -> Result<ModifierId, UnknownName> {
            Stats::modifier(self.world, self.package, name.as_str())
                .ok_or_else(|| UnknownName::new(NameKind::Modifier, name))
        }

        fn track(&self, name: &DeclaredName) -> Result<TrackId, UnknownName> {
            self.view
                .names()
                .named(name.as_str())
                .ok_or_else(|| UnknownName::new(NameKind::Track, name))
        }

        fn tag(&self, name: &DeclaredName) -> Result<Tag, UnknownName> {
            self.view
                .names()
                .tags()
                .named(name.as_str())
                .ok_or_else(|| UnknownName::new(NameKind::Tag, name))
        }

        fn package_type(&self, name: &DeclaredName) -> Result<UnitType, UnknownName> {
            let scope = TypeScope::of_package(self.package);
            self.view
                .names()
                .named((scope, name.as_str()))
                .ok_or_else(|| UnknownName::new(NameKind::UnitType, name))
        }
    }
}
