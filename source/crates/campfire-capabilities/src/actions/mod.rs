use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use campfire_sim::{Capability, SimSet};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::actions_api::ActionsApi;
use crate::actions::actions_column::{ActionsColumn, RowParts};
use crate::actions::effect_lists::EffectLists;
use crate::actions::effect_queues::EffectQueues;
use crate::actions::holds::Holds;
use crate::actions::slot_kinds::SlotKinds;
use crate::actions::slot_times::SlotTimes;
use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::stats::StatsSet;
use crate::stats::param_book::ParamBook;

pub(crate) mod action;
pub(crate) mod action_book;
pub(crate) mod action_call;
pub(crate) mod action_data;
pub(crate) mod action_data_field;
pub(crate) mod action_kind;
pub(crate) mod action_names;
pub(crate) mod action_parts;
pub(crate) mod action_range;
pub(crate) mod action_slots;
pub(crate) mod action_target;
pub(crate) mod actions_api;
pub(crate) mod actions_column;
pub(crate) mod actions_effect;
pub(crate) mod amount;
pub(crate) mod capability_does;
pub(crate) mod channel_call;
pub(crate) mod checked_action;
pub(crate) mod construct_data;
pub(crate) mod cost_target;
pub(crate) mod delivery;
pub(crate) mod delivery_data;
pub(crate) mod does;
pub(crate) mod effect_data;
pub(crate) mod effect_lists;
pub(crate) mod effect_names;
pub(crate) mod effect_queues;
pub(crate) mod error;
pub(crate) mod fan;
pub(crate) mod gather_spec;
pub(crate) mod holds;
pub(crate) mod in_progress;
pub(crate) mod kind_data;
pub(crate) mod kind_spec;
pub(crate) mod launch_id;
pub(crate) mod lists_of;
pub(crate) mod payer;
pub(crate) mod placement_data;
pub(crate) mod purse;
pub(crate) mod range_field;
pub(crate) mod rank_fields;
pub(crate) mod rank_values;
pub(crate) mod ready_waits;
pub(crate) mod requires_data;
pub(crate) mod slot_aim;
pub(crate) mod slot_kind;
pub(crate) mod slot_kind_data;
pub(crate) mod slot_kinds;
pub(crate) mod slot_times;
pub(crate) mod targeting;
pub(crate) mod targets;
pub(crate) mod weapon;

/// The core's actions: every action a match loads, and the slots units hold them in.
#[derive(Debug)]
pub struct Actions;

/// The systems of the action pipeline, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ActionsSet {
    /// In `SimSet::Act`, after attacks start: what each unit was ordered starts, each kind's
    /// orders by the capability that runs the kind.
    Start,
    /// In `SimSet::Inputs`, once the tick's orders and expiries are in: passives hold as the
    /// slots stand.
    HoldAtInputs,
    /// In `SimSet::Resolve`, once the tick's damage is dealt and before units die: passives hold
    /// as the slots stand.
    HoldAtResolve,
    /// In `SimSet::Hit`, once the attacks going off paid their toggles and before their events:
    /// holds follow the toggles an unpaid cost turned off.
    HoldAtStrike,
}

impl Declares for Actions {
    const MODULE: Option<Capability> = None;

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<ActionSlots>();
        d.component::<SlotTimes>();
        d.require::<ActionSlots, SlotTimes>();
        d.book::<ActionBook>();
        d.book::<ParamBook>();
        d.book::<EffectLists>();
        d.column::<ActionsColumn, RowParts>(ActionsColumn::fill_row);
        d.api(ActionsApi::register);
    }
}

impl Actions {
    /// Adds the actions to a match whose core is installed, with none loaded yet, and, in a match
    /// with combat, the holds of every action: its charges as each tick starts, and its passives
    /// then and as each stage that changes ranks or cooldowns ends. A match with no combat holds
    /// none, as every action's unit target is one combat finds; the start of an action is the
    /// abilities'.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        cx.world.insert_resource(SlotKinds::default());
        cx.world.insert_resource(EffectQueues::default());
        if cx.has(Capability::Combat) {
            Actions::schedule(cx.schedule);
        }
    }

    fn schedule(schedule: &mut Schedule) {
        schedule.add_systems((
            Holds::hold_charges
                .in_set(SimSet::Inputs)
                .in_set(ActionsSet::HoldAtInputs),
            Holds::hold_passives
                .in_set(SimSet::Inputs)
                .in_set(ActionsSet::HoldAtInputs)
                .after(StatsSet::Expire)
                .after(Holds::hold_charges),
            Holds::hold_passives
                .in_set(SimSet::Hit)
                .in_set(ActionsSet::HoldAtStrike),
            Holds::hold_passives
                .in_set(SimSet::Resolve)
                .in_set(ActionsSet::HoldAtResolve),
            Holds::hold_passives.in_set(SimSet::Vision),
        ));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_common::Ticks;
    use campfire_math::Num;

    use crate::actions::Actions;
    use crate::actions::action::{Action, Aim};
    use crate::actions::action_book::ActionBook;
    use crate::actions::action_range::ActionRange;
    use crate::actions::actions_column::ActionsColumn;
    use crate::actions::gather_spec::GatherSpec;
    use crate::actions::kind_spec::KindSpec;
    use crate::actions::rank_values::RankValues;
    use crate::scripts::hook_set::HookSet;
    use crate::stats::pool_cost::PoolCost;
    use crate::units::action_id::ActionId;
    use crate::units::filter::Filter;
    use crate::units::view::View;

    impl Actions {
        /// The action `name` of `package`, as the match loaded it.
        pub fn named(world: &World, package: u16, name: &str) -> Option<ActionId> {
            world.resource::<ActionBook>().named(package, name)
        }

        /// Adds a gather of `spec` from the nodes `aim` selects, within `range` meters, a trip of
        /// `time`, to the match's action book.
        pub(crate) fn add_gather(
            world: &mut World,
            spec: GatherSpec,
            aim: Filter,
            range: Num,
            time: Ticks,
        ) -> ActionId {
            Actions::push(
                world,
                Action {
                    package: 0,
                    name: "gather".into(),
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

        /// Adds `action` to the match's action book, and shares the book with the script view.
        pub(crate) fn push(world: &mut World, action: Action) -> ActionId {
            let mut book = world.resource_mut::<ActionBook>();
            let id = book.push(action);
            let book = book.clone();
            ActionsColumn::share(world.non_send::<View>(), book);
            id
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use std::num::NonZeroU8;

    use bevy_ecs::world::Mut;
    use bevy_ecs::world::World;
    use campfire_common::Ticks;
    use campfire_math::Num;
    use campfire_script::ScriptId;
    use campfire_sim::TickRate;

    use crate::actions::Actions;
    use crate::actions::action::{Action, Aim};
    use crate::actions::action_book::ActionBook;
    use crate::actions::action_data::ActionData;
    use crate::actions::action_names::ActionNames;
    use crate::actions::action_parts::ActionParts;
    use crate::actions::action_range::ActionRange;
    use crate::actions::actions_column::ActionsColumn;
    use crate::actions::cost_target::CostTarget;
    use crate::actions::delivery::{Delivery, DeliveryShape};
    use crate::actions::effect_lists::EffectLists;
    use crate::actions::effect_names::EffectNames;
    use crate::actions::fan::Fan;
    use crate::actions::kind_spec::KindSpec;
    use crate::actions::rank_values::RankValues;
    use crate::actions::weapon::Weapon;
    use crate::players::resource_amount::ResourceAmount;
    use crate::progression::progression_column::ProgressionColumn;
    use crate::projectiles::projectile_spec::ProjectileSpec;
    use crate::scripts::hook_set::HookSet;
    use crate::scripts::script_book::ScriptBook;
    use crate::stats::Stats;
    use crate::stats::modifier_book::ModifierBook;
    use crate::stats::param_book::ParamBook;
    use crate::stats::pool_cost::PoolCost;
    use crate::stats::pool_id::PoolId;
    use crate::stats::stat_id::StatId;
    use crate::stats::stats_column::StatsColumn;
    use crate::units::action_id::ActionId;
    use crate::units::by_type::ByType;
    use crate::units::filter::Filter;
    use crate::units::modifier_id::ModifierId;
    use crate::units::tag::Tag;
    use crate::units::track_id::TrackId;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_type::UnitType;
    use crate::units::view::View;
    use crate::values::damage_kind::DamageKind;
    use crate::values::declared_name::DeclaredName;
    use crate::values::error::TimeTooLarge;
    use crate::values::filter_data::FilterData;
    use crate::values::param::Param;
    use crate::values::stat::Stat;

    /// A weapon tests arm units with: what it aims at, its range, its windup, the type of the
    /// homing projectile it fires, if it fires one, the places among its unit's stats of its
    /// rate and its damage, and its cost in pools and in a player resource.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct TestWeapon {
        pub(crate) aim: Filter,
        pub(crate) range: ActionRange,
        pub(crate) windup: Ticks,
        pub(crate) projectile: Option<UnitType>,
        pub(crate) rate: StatId,
        pub(crate) damage: StatId,
        pub(crate) cost: PoolCost,
        pub(crate) resource_cost: Option<ResourceAmount>,
    }

    impl TestWeapon {
        /// A melee weapon at `aim` within `range` that winds up `windup`, of the first two stats'
        /// rate and damage, costing nothing.
        pub(crate) fn new(aim: Filter, range: ActionRange, windup: Ticks) -> TestWeapon {
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
        pub(crate) fn add_weapon(world: &mut World, weapon: TestWeapon) -> ActionId {
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
                Action {
                    package: 0,
                    name: "weapon".into(),
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
        pub(crate) fn load_effect_lists(
            world: &mut World,
            action: ActionId,
            package: u16,
            data: &ActionData,
        ) {
            let view = world.non_send::<View>().clone();
            let mut lists = world.remove_resource::<EffectLists>().unwrap();
            let names = MatchEffectNames::new(world, &view, action, package);
            lists.push(action, data, &names);
            world.insert_resource(lists);
        }

        /// Loads the action `name` of `package`, of `ranks` ranks, into the match, which the
        /// package load checked, with its compiled script exactly when its data names one: its
        /// capability fields at each rank, times in milliseconds as ticks at the match's rate,
        /// rounded up.
        pub(crate) fn load(
            world: &mut World,
            package: u16,
            name: &str,
            data: &ActionData,
            script: Option<ScriptId>,
            ranks: u8,
        ) -> Result<ActionId, TimeTooLarge> {
            let rate = *world.resource::<TickRate>();
            let view = world.non_send::<View>().clone();
            let names = MatchNames::new(world, &view, world.resource::<ModifierBook>());
            let parts = ActionParts::of(data, package, ranks, rate, &names)?;
            let id = world.resource_scope(|world, mut actions: Mut<'_, ActionBook>| {
                let scripts = world.resource::<ScriptBook>();
                actions.load(scripts, package, name, data, script, parts)
            });
            let places = Stats::stat_places(world, data.params.values().flat_map(Param::stats));
            Stats::load_action_params(world, id, &data.params, |stat| places[stat]);
            let book = world.resource::<ActionBook>().clone();
            ActionsColumn::share(world.non_send::<View>(), book);
            Ok(id)
        }

        /// Adds a train of `unit_type` that takes `time` and costs `resource_cost`, no pool and no
        /// cooldown, to the match's action book.
        pub(crate) fn add_train(
            world: &mut World,
            unit_type: UnitType,
            time: Ticks,
            resource_cost: Option<ResourceAmount>,
        ) -> ActionId {
            Actions::push(
                world,
                Action {
                    package: 0,
                    name: "train".into(),
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
        pub(crate) fn add_build(
            world: &mut World,
            unit_type: UnitType,
            range: impl Into<Num>,
            time: Ticks,
            resource_cost: Option<ResourceAmount>,
        ) -> ActionId {
            let range = range.into();
            Actions::push(
                world,
                Action {
                    package: 0,
                    name: "build".into(),
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

    /// The names of an action's effect lists as a match's world resolves them: its view, its param
    /// book, and the modifiers of the action's package.
    #[derive(Debug)]
    pub(crate) struct MatchEffectNames<'w> {
        world: &'w World,
        view: &'w View,
        action: ActionId,
        package: u16,
    }

    impl<'w> MatchEffectNames<'w> {
        pub(crate) const fn new(
            world: &'w World,
            view: &'w View,
            action: ActionId,
            package: u16,
        ) -> Self {
            MatchEffectNames {
                world,
                view,
                action,
                package,
            }
        }
    }

    impl EffectNames for MatchEffectNames<'_> {
        fn param(&self, name: &DeclaredName) -> usize {
            self.world
                .resource::<ParamBook>()
                .actions()
                .named(self.action.index(), name.as_str())
                .expect("the load checked an effect's param")
        }

        fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
            self.view
                .damage_kind_named(name.as_str())
                .expect("the load checked an effect's damage kind")
        }

        fn pool(&self, name: &DeclaredName) -> PoolId {
            StatsColumn::pool_id_named(self.view, name.as_str())
                .expect("the load checked an effect's pool")
        }

        fn modifier(&self, name: &DeclaredName) -> ModifierId {
            Stats::modifier(self.world, self.package, name.as_str())
                .expect("the load checked an effect's modifier")
        }

        fn track(&self, name: &DeclaredName) -> TrackId {
            ProgressionColumn::track_named(self.view, name.as_str())
                .expect("the load checked an effect's track")
        }

        fn tag(&self, name: &DeclaredName) -> Tag {
            self.view
                .tag_named(name.as_str())
                .expect("the load checked an effect's tag")
        }

        fn package_type(&self, name: &DeclaredName) -> UnitType {
            let scope = TypeScope::of_package(self.package);
            self.view
                .types_mut()
                .named(scope, name.as_str())
                .expect("the load checked an effect's unit type")
        }
    }

    /// The names of an action's data as a match's world resolves them: its view, its book of
    /// modifiers, and the world's projectile specs.
    #[derive(Debug)]
    pub(crate) struct MatchNames<'w> {
        world: &'w World,
        view: &'w View,
        modifiers: &'w ModifierBook,
    }

    impl<'w> MatchNames<'w> {
        pub(crate) const fn new(
            world: &'w World,
            view: &'w View,
            modifiers: &'w ModifierBook,
        ) -> Self {
            MatchNames {
                world,
                view,
                modifiers,
            }
        }
    }

    impl ActionNames for MatchNames<'_> {
        fn stat(&self, stat: &Stat) -> StatId {
            self.view
                .stat_index(stat)
                .expect("the load checked the stats")
        }

        fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
            self.view
                .damage_kind_named(name.as_str())
                .expect("the load checked the damage kind")
        }

        fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget> {
            self.view.cost_target(name.as_str())
        }

        fn filter(&self, filter: &FilterData) -> Filter {
            self.view
                .resolve_filter(filter)
                .expect("the load checked the filter's tags")
        }

        fn modifier(&self, package: u16, name: &DeclaredName) -> ModifierId {
            self.modifiers
                .named(package, name.as_str())
                .expect("the load checked the modifier")
        }

        fn homes(&self, package: u16, name: &DeclaredName) -> bool {
            let unit_type = self.unit_type(package, name);
            let specs = self.world.resource::<ByType<ProjectileSpec>>();
            specs
                .get(unit_type)
                .expect("a test loads a projectile type's spec before its action")
                .homing
        }

        fn unit_type(&self, package: u16, name: &DeclaredName) -> UnitType {
            let scope = TypeScope::of_package(package);
            self.view
                .types_mut()
                .named(scope, name.as_str())
                .expect("a test loads an action's unit type before the action")
        }
    }
}
