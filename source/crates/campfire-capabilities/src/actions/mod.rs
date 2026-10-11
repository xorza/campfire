use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::world::World;
use campfire_sim::{SimSet, StateRegistry};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::actions_column::{ActionsColumn, RowParts};
use crate::actions::effect_lists::EffectLists;
use crate::actions::effect_queues::EffectQueues;
use crate::actions::holds::Holds;
use crate::actions::slot_kinds::SlotKinds;
use crate::scripts::ctx::Ctx;
use crate::state_types::StateTypes;
use crate::stats::StatsSet;
use crate::units::view::View;

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

impl Actions {
    /// Lists the state types it adds (design 14, D9).
    pub(crate) fn state_types<T: StateTypes>(types: &mut T) {
        types.component::<ActionSlots>();
    }

    /// Adds the actions to a match whose core is installed, with none loaded yet, and its column
    /// to the script view.
    pub(crate) fn install(world: &mut World, registry: &mut StateRegistry) {
        let view = world.non_send::<View>().clone();
        view.add_column(ActionsColumn::default());
        view.add_source::<RowParts, _>(world, ActionsColumn::fill_row);
        world.insert_resource(ActionBook::default());
        world.insert_resource(SlotKinds::default());
        world.insert_resource(EffectQueues::default());
        if world.contains_non_send::<Ctx>() {
            world.insert_resource(EffectLists::default());
        }
        Self::state_types(registry);
    }

    /// Adds to `schedule` the holds of every action: its charges as each tick starts, and its
    /// passives then and as each stage that changes ranks or cooldowns ends. Combat calls it, as
    /// every action's unit target is one combat finds; the start of an action is the abilities'.
    pub(crate) fn schedule(schedule: &mut Schedule) {
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

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;

    use crate::actions::Actions;
    use crate::actions::action_book::ActionBook;
    use crate::units::action_id::ActionId;

    impl Actions {
        /// The action `name` of `package`, as the match loaded it.
        pub fn action(world: &World, package: u16, name: &str) -> Option<ActionId> {
            world.resource::<ActionBook>().named(package, name)
        }
    }
}

#[cfg(test)]
pub(crate) mod loads {
    use crate::actions::Actions;
    use crate::actions::action_book::ActionBook;
    use crate::actions::action_data::ActionData;
    use crate::actions::action_names::ActionNames;
    use crate::actions::action_parts::ActionParts;
    use crate::actions::actions_column::ActionsColumn;
    use crate::actions::cost_target::CostTarget;
    use crate::projectiles::projectile_spec::ProjectileSpec;
    use crate::scripts::script_book::ScriptBook;
    use crate::stats::modifier_book::ModifierBook;
    use crate::stats::param_book::ParamBook;
    use crate::stats::stat_book::StatBook;
    use crate::stats::stat_id::StatId;
    use crate::units::action_id::ActionId;
    use crate::units::by_type::ByType;
    use crate::units::filter::Filter;
    use crate::units::modifier_id::ModifierId;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_type::UnitType;
    use crate::units::view::View;
    use crate::values::damage_kind::DamageKind;
    use crate::values::declared_name::DeclaredName;
    use crate::values::error::TimeTooLarge;
    use crate::values::filter_data::FilterData;
    use crate::values::param::Param;
    use crate::values::stat::Stat;
    use bevy_ecs::world::{Mut, World};
    use campfire_script::ScriptId;
    use campfire_sim::TickRate;

    impl Actions {
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
            let names = MatchNames {
                world,
                view: &view,
                modifiers: world.resource::<ModifierBook>(),
            };
            let parts = ActionParts::of(data, package, ranks, rate, &names)?;
            let id = world.resource_scope(|world, mut actions: Mut<'_, ActionBook>| {
                let scripts = world.resource::<ScriptBook>();
                actions.load(scripts, package, name, data, script, parts)
            });
            let places = StatBook::places(world, data.params.values().flat_map(Param::stats));
            ParamBook::load_action(world, id, &data.params, |stat| places[stat]);
            let book = world.resource::<ActionBook>().clone();
            ActionsColumn::share(world.non_send::<View>(), book);
            Ok(id)
        }
    }

    /// The names of an action's data as a match's world resolves them: its view, its book of
    /// modifiers, and the world's projectile specs.
    #[derive(Debug)]
    struct MatchNames<'w> {
        world: &'w World,
        view: &'w View,
        modifiers: &'w ModifierBook,
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
