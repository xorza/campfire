use bevy_ecs::query::ROQueryItem;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::World;

use campfire_sim::{SimSet, SimTick, StableId, StateRegistry, TickRate};

use crate::actions::action_book::ActionBook;
use crate::actions::effect_lists::EffectLists;
use crate::actions::effect_queues::EffectQueues;
use crate::actions::slot_kinds::SlotKinds;
use crate::scripts::ctx::Ctx;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::lifetime::Hold;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::param_book::ParamBook;

use crate::actions::action_slots::{ActionSlots, SlotCharges};
use crate::actions::actions_column::ActionsColumn;

use crate::stats::StatsSet;
use crate::stats::applier::Applier;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::modifiers::Modifiers;
use crate::stats::param_sources::ParamSources;

use crate::stats::stat_book::StatBook;

use crate::units::row_fill::RowFill;
use crate::units::script_view::View;

pub(crate) mod action;
pub(crate) mod action_book;
pub(crate) mod action_data;
pub(crate) mod action_data_field;
pub(crate) mod action_kind;
pub(crate) mod action_names;
pub(crate) mod action_parts;
pub(crate) mod action_slots;
pub(crate) mod action_target;
pub(crate) mod actions_api;
pub(crate) mod actions_column;
pub(crate) mod cost_target;
pub(crate) mod delivery;
pub(crate) mod delivery_data;
pub(crate) mod effect_data;
pub(crate) mod effect_lists;
pub(crate) mod effect_names;
pub(crate) mod effect_queues;
pub(crate) mod error;
pub(crate) mod fan;
pub(crate) mod kind_spec;
pub(crate) mod purse;
pub(crate) mod range;
pub(crate) mod rank_values;
pub(crate) mod slot_kind;
pub(crate) mod slot_kinds;
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
    /// Adds the actions to a match whose core is installed, with none loaded yet, and its column
    /// to the script view.
    pub(crate) fn install(world: &mut World, registry: &mut StateRegistry) {
        let view = world.non_send::<View>().clone();
        view.add_column(ActionsColumn::default());
        view.add_source::<RowParts>(world, fill_row);
        world.insert_resource(ActionBook::default());
        world.insert_resource(SlotKinds::default());
        world.insert_resource(EffectQueues::default());
        if world.contains_non_send::<Ctx>() {
            world.insert_resource(EffectLists::default());
        }
        registry.register_component::<ActionSlots>();
    }

    /// Adds the start of every action to `schedule`, in Act, and the passives of every action,
    /// as each stage that changes ranks or cooldowns ends: combat's to run, as every action's unit
    /// target is one combat finds.
    pub(crate) fn schedule(schedule: &mut Schedule) {
        schedule.add_systems((
            hold_charges
                .in_set(SimSet::Inputs)
                .in_set(ActionsSet::HoldAtInputs),
            hold_passives
                .in_set(SimSet::Inputs)
                .in_set(ActionsSet::HoldAtInputs)
                .after(StatsSet::Expire)
                .after(hold_charges),
            hold_passives
                .in_set(SimSet::Hit)
                .in_set(ActionsSet::HoldAtStrike),
            hold_passives
                .in_set(SimSet::Resolve)
                .in_set(ActionsSet::HoldAtResolve),
            hold_passives.in_set(SimSet::Vision),
        ));
    }
}

/// Keeps each unit's charges as its slots stand, as each tick starts, after the orders that learn
/// ranks: a slot that came to a rank of an action with charges fills, and each charge whose time
/// came comes back. A slot changes only when its charges do, as the slots replicate.
fn hold_charges(
    actions: Res<'_, ActionBook>,
    tick: Res<'_, SimTick>,
    mut units: Query<'_, '_, &mut ActionSlots>,
    mut due: Local<'_, Vec<(u8, Option<SlotCharges>)>>,
) {
    let now = tick.start();
    for mut slots in &mut units {
        due.clear();
        due.extend(slots.charges_due(&actions, now));
        for &(slot, charges) in &*due {
            slots.set_charges(slot, charges);
        }
    }
}

/// Keeps each unit's passives and holds as its slots stand: the passive of each action with a
/// rank, and with `passive_while_ready` off cooldown, and the `hold` of each action whose toggle
/// is on or whose channel runs, each from the unit itself at the action's rank, applied again when the rank changes;
/// and none other. It runs as each tick starts, after the casts resolve and the attacks strike,
/// and after the mode's calls, which learn ranks. A passive's or a hold's params are the match's
/// param book's.
fn hold_passives(
    actions: Res<'_, ActionBook>,
    book: Option<Res<'_, ModifierBook>>,
    stats: Option<Res<'_, StatBook>>,
    (tick, rate): (Res<'_, SimTick>, Res<'_, TickRate>),
    params: Res<'_, ParamBook>,
    sources: ParamSources<'_, '_>,
    mut units: Query<'_, '_, (&StableId, &ActionSlots, &mut Modifiers, &mut ModifierClocks)>,
) {
    if stats.is_none() {
        return;
    }
    let Some(book) = book else {
        return;
    };
    let now = tick.start();
    for (&id, slots, modifiers, clocks) in &mut units {
        let mut carried = CarriedMut::new(modifiers, clocks);
        for (index, slot) in (0..).zip(slots.iter()) {
            let Some(ability) = slot.action else {
                continue;
            };
            let action = actions
                .get(ability)
                .expect("a slot's action is in the book");
            let mut keep = |modifier, hold, holds: bool| {
                let held = carried
                    .modifiers()
                    .get(modifier, Some(id))
                    .filter(|instance| instance.lifetime.held_by(hold))
                    .map(|instance| instance.rank);
                if !holds {
                    if held.is_some() {
                        carried.release(modifier, Some(id), hold);
                    }
                    return;
                }
                if held == Some(slot.rank) {
                    return;
                }
                let applier = Applier {
                    source: Some(id),
                    ability: Some(ability),
                    rank: slot.rank,
                    hold: Some(hold),
                };
                let source = sources.get(id);
                let param = |place: &ParamPlace| {
                    let (ability, rank) = (Some(ability), slot.rank);
                    params.modifier_param(modifier, ability, rank, place, source.as_ref())
                };
                carried.apply(book.application(modifier, applier, None, now, *rate, param));
            };
            if let Some(passive) = action.passive {
                let holds = slot.rank > 0 && (!passive.while_ready || slot.ready_at <= now);
                keep(passive.modifier, Hold::Passive, holds);
            }
            if let Some(hold) = action.hold {
                let runs = slot.toggle.is_some() || slots.channeling() == Some(index);
                keep(hold, Hold::Running, runs);
            }
        }
    }
}

/// The part of a unit the actions read into its row: its slots.
type RowParts = Option<&'static ActionSlots>;

/// Adds a unit's actions to the actions' column of the script view.
fn fill_row(slots: ROQueryItem<'_, '_, RowParts>, fill: &mut RowFill<'_>) {
    fill.column::<ActionsColumn>().push(slots);
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
    use crate::actions::error::ActionError;
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
    use crate::units::script_view::View;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_type::UnitType;
    use crate::values::damage_kind::DamageKind;
    use crate::values::declared_name::DeclaredName;
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
        ) -> Result<ActionId, ActionError> {
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
