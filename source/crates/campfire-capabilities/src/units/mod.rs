use bevy_ecs::change_detection::CheckChangeTicks;
use bevy_ecs::observer::On;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::system::NonSend;
use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;
use campfire_script::{ScriptError, ScriptHost, ScriptId};
use campfire_sim::{Breach, Capability, Position, SimEdge, SimSet, TickRate};

use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::declare::kind_visitor::KindVisitor;
use crate::geometry::bounds::Bounds;
use crate::geometry::metric::Metric;
use crate::players::kind_parts::KindParts;
use crate::players::match_entity::MatchEntity;
use crate::players::player::Player;
use crate::players::players_world::PlayersWorld;
use crate::scripts::core_api::CoreApi;
use crate::scripts::ctx::Ctx;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::scripts::scripts_call::ScriptsCall;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::delivery_entity::DeliveryEntity;
use crate::units::facing::Facing;
use crate::units::forced_move::ForcedMove;
use crate::units::lifespan::Lifespan;
use crate::units::move_step::MoveStep;
use crate::units::new_unit::NewUnit;
use crate::units::new_unit_states::NewUnitStates;
use crate::units::owner::Owner;
use crate::units::relations::Relations;
use crate::units::spawn_point::SpawnPoint;
use crate::units::status_tags::StatusTags;
use crate::units::team::Team;
use crate::units::type_origins::TypeOrigins;
use crate::units::unit::Unit;
use crate::units::unit_entity::UnitEntity;
use crate::units::unit_state::{SyncAll, SyncNone, SyncOwner, SyncTeam, UnitState};
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_ticks::UnitTicks;
use crate::units::unit_type::UnitType;
use crate::units::units_api::UnitsApi;
use crate::units::units_call::UnitsCall;
use crate::units::units_column::{StateRowParts, UnitsColumn};
use crate::units::view::View;

pub(crate) mod action_id;
pub(crate) mod bits256;
pub(crate) mod block;
pub(crate) mod body;
pub(crate) mod body_form;
pub(crate) mod body_grid;
pub(crate) mod by_type;
pub(crate) mod collision_data;
pub(crate) mod dead;
pub(crate) mod delivery_entity;
pub(crate) mod engine_tag;
pub(crate) mod entity_kind;
pub(crate) mod facing;
pub(crate) mod filter;
pub(crate) mod forced_move;
pub(crate) mod hit_handle;
pub(crate) mod kept_rows;
pub(crate) mod kind_filters;
pub(crate) mod layer;
pub(crate) mod lifespan;
pub(crate) mod living_unit;
pub(crate) mod modifier_id;
pub(crate) mod move_step;
pub(crate) mod new_unit;
pub(crate) mod new_unit_states;
pub(crate) mod owner;
pub(crate) mod path_id;
pub(crate) mod player_units;
pub(crate) mod predicting;
pub(crate) mod relations;
pub(crate) mod row_fill;
pub(crate) mod row_marks;
pub(crate) mod row_parts;

mod source_reads;
pub(crate) mod spawn_at;
pub(crate) mod spawn_point;
pub(crate) mod spawn_target;
pub(crate) mod spawner;
pub(crate) mod status_tags;
pub(crate) mod tag;
pub(crate) mod tag_book;
pub(crate) mod tag_data;
pub(crate) mod tag_effect;
pub(crate) mod tag_properties;
pub(crate) mod tag_property;
pub(crate) mod tag_set;
pub(crate) mod target_index;
pub(crate) mod team;
pub(crate) mod team_set;
pub(crate) mod teams;
pub(crate) mod track_id;
pub(crate) mod type_origins;
pub(crate) mod type_scope;
pub(crate) mod unit;
pub(crate) mod unit_core;
pub(crate) mod unit_entity;
pub(crate) mod unit_row;
pub(crate) mod unit_rows;
pub(crate) mod unit_state;
pub(crate) mod unit_state_access;
pub(crate) mod unit_state_book;
pub(crate) mod unit_states;
pub(crate) mod unit_tags;
pub(crate) mod unit_ticks;
pub(crate) mod unit_type;
pub(crate) mod unit_type_data;
pub(crate) mod unit_types;
pub(crate) mod units_api;
pub(crate) mod units_call;
pub(crate) mod units_column;
pub(crate) mod units_world;
pub(crate) mod view;
pub(crate) mod view_column;
pub(crate) mod view_names;

/// The core's systems, for the capabilities above it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum UnitsSet {
    /// In `SimSet::Inputs`: every pool starts full, and the last tick's writes to units that
    /// never spawned clear.
    BeginTick,
}

/// The core under every capability's scripts: unit types, the script host they run in, and the
/// units as scripts see them, with the queries on them. Every match installs it before its
/// capabilities.
#[derive(Debug)]
pub struct Units;

impl Declares for Units {
    const MODULE: Option<Capability> = None;

    fn declare<D: Declarations>(d: &mut D) {
        Units::kinds(d);
        d.component::<Body>();
        d.component::<Dead>();
        d.component::<Facing>();
        d.component::<ForcedMove>();
        d.component::<Lifespan>();
        d.component::<MatchEntity>();
        d.component::<MoveStep>();
        d.sim_component::<Position>();
        d.component::<Owner>();
        d.component::<SpawnPoint>();
        d.component::<StatusTags>();
        d.component::<Team>();
        d.component::<UnitEntity>();
        d.component::<UnitType>();
        d.component::<UnitState<SyncNone>>();
        d.component::<UnitState<SyncOwner>>();
        d.component::<UnitState<SyncTeam>>();
        d.component::<UnitState<SyncAll>>();
        d.derived::<DeliveryEntity>();
        d.derived::<UnitTags>();
        d.component::<Relations>();
        d.check::<Position>(Units::within_bounds);
        d.host::<Position, UnitEntity>();
        d.require::<UnitEntity, Position>();
        d.require::<UnitEntity, Team>();
        d.require::<Player, Team>();
        d.check::<MatchEntity>(Units::match_spawned);
        d.book::<UnitStateBook>();
        d.book::<TypeOrigins>();
        d.column::<UnitsColumn, StateRowParts>(UnitsColumn::fill_state);
        d.call_part::<UnitsCall>();
        d.call_part::<ScriptsCall>();
        d.api(CoreApi::register);
        d.api(Unit::register);
        d.api(NewUnit::register);
        d.api(UnitsApi::register);
        d.api(View::register_queries);
    }
}

impl Units {
    /// Visits each kind of entity, which state types name as their host.
    pub(crate) fn kinds<V: KindVisitor>(v: &mut V) {
        v.kind::<UnitEntity>();
        v.kind::<DeliveryEntity>();
        v.kind::<MatchEntity>();
        v.kind::<Player>();
    }

    /// Adds the core to a match, on a planar map until the mode sets its own. With scripts, in
    /// Inputs, every pool starts full and the last tick's failures clear.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        let world = &mut *cx.world;
        world.insert_resource(NewUnitStates::default());
        world.insert_resource(Metric::default());
        // The world's bounds until a map's take their place.
        world.insert_resource(Bounds::WORLD);
        world.add_observer(|_: On<'_, '_, CheckChangeTicks>, view: NonSend<'_, View>| {
            view.refill_next();
        });
        // A client holds its deliveries unpredicted, so the system finds none there.
        cx.schedule
            .add_systems(UnitTags::give_delivery_tags.in_set(SimEdge::Start));
        if cx.scripts() {
            cx.schedule.add_systems((
                UnitTicks::begin_tick
                    .in_set(SimSet::Inputs)
                    .in_set(UnitsSet::BeginTick),
                UnitTicks::end_lifespans.in_set(SimSet::Vision),
            ));
        }
    }

    /// Whether every unit stands within the match's bounds, as every system keeps it, and as
    /// vision and navigation index the map's cells by it; the first that does not.
    fn within_bounds(world: &World) -> Result<(), Breach> {
        let bounds = *world.resource::<Bounds>();
        Breach::first(world, |entity| {
            world
                .get::<Position>(entity)
                .is_some_and(|&at| !bounds.contains(at))
        })
    }

    /// Whether the match's entity stands, as every match that started spawns it, so no system
    /// finds it missing.
    fn match_spawned(world: &World) -> Result<(), Breach> {
        Breach::of_match(world.match_entity().is_some())
    }

    /// Compiles `source` in the match's script host, once for every capability that runs it.
    pub fn compile(world: &mut World, source: &str) -> Result<ScriptId, ScriptError> {
        world.non_send_mut::<ScriptHost>().compile(source)
    }

    /// Opens a match for the modules' installs: the script view, and with `budgets`, the script
    /// host the scripts run in within them, and the context of their calls. A client runs no
    /// scripts.
    pub(crate) fn open(world: &mut World, budgets: Option<ScriptBudgets>) {
        let rate = *world.resource::<TickRate>();
        let view = View::new(rate);
        world.init_resource::<KindParts<MatchEntity>>();
        world.init_resource::<KindParts<Player>>();
        if let Some(budgets) = budgets {
            let ctx = Ctx::new(view.clone());
            let mut host = ScriptHost::new(budgets.limits().per_call);
            host.engine_mut()
                .set_default_tag(Dynamic::from(ctx.clone()));
            world.insert_non_send(ctx);
            world.insert_non_send(host);
            world.insert_non_send(ScriptFailures::default());
            world.insert_resource(budgets);
        }
        world.insert_non_send(view);
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;

    use crate::units::Units;
    use crate::units::team::Team;
    use crate::units::units_world::UnitsWorldCrate;
    use crate::values::relation::Relation;

    impl Units {
        /// Sets how the two teams of the match regard each other, as `ctx.set_relation`
        /// does, their vision as it was.
        pub fn set_relation(world: &mut World, of: Team, other: Team, relation: Relation) {
            world
                .relations_mut()
                .set_keeping_vision(of, other, relation);
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::change_detection::Mut;
    use bevy_ecs::resource::Resource;
    use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
    use bevy_ecs::system::{Query, Res};
    use bevy_ecs::world::World;
    use campfire_common::Tick;
    use campfire_script::rhai::Dynamic;
    use campfire_script::{Budget, ScriptError, ScriptHost, ScriptId};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{IdAllocator, Position, SimSet, SimTick, SimUpdate, StableId};

    use crate::scripts::ctx::Ctx;
    use crate::scripts::error::CallError;
    use crate::scripts::script_book::ScriptBook;
    use crate::stats::stats_column::StatsColumn;
    use crate::units::Units;
    use crate::units::block::Block;
    use crate::units::tag_book::TagBook;
    use crate::units::tag_data::TagData;
    use crate::units::team::Team;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_core::UnitCore;
    use crate::units::unit_tags::UnitTags;
    use crate::units::unit_type::UnitType;
    use crate::units::unit_type_data::UnitTypeData;
    use crate::units::units_column::UnitsColumn;
    use crate::units::view::View;
    use crate::values::declared_name::DeclaredName;
    use crate::values::scalar::Scalar;

    impl Units {
        /// A unit of `team` at `pos` with `parts`, of the match's own type, spawned as `spawn_of`
        /// spawns one.
        pub(crate) fn spawn(
            world: &mut World,
            pos: Position,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            let unit_type = Units::test_type(world);
            Units::spawn_of(world, unit_type, pos, team, parts)
        }

        /// A unit of `unit_type` and `team` at `pos`, of the next stable id, spawned as a mode's unit
        /// and a delivery are: `parts`, then its core, with its type's tags and its script state from
        /// the match's books. The first spawn seals the match's unit types, as a match loads them all
        /// before any unit spawns.
        pub(crate) fn spawn_of(
            world: &mut World,
            unit_type: UnitType,
            pos: Position,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            let id = world.resource_mut::<IdAllocator>().allocate();
            let core = UnitCore {
                id,
                pos,
                unit_type,
                team,
                owner: None,
            };
            Units::spawn_core(world, core, parts)
        }

        /// The unit of `core`, with `parts`, spawned as a mode's unit and a delivery are; the first
        /// spawn seals the match's unit types.
        pub(crate) fn spawn_core(
            world: &mut World,
            core: UnitCore,
            parts: impl Bundle,
        ) -> StableId {
            Units::seal_types(world);
            core.spawn(world, parts);
            core.id
        }

        /// The type of the units a test spawns with no type of its own: one of no tag, param or
        /// state field, which sealing the match's unit types loads.
        pub(crate) fn test_type(world: &mut World) -> UnitType {
            Units::seal_types(world);
            world
                .get_resource::<TestUnitType>()
                .expect("a test that puts its own tag book spawns units of its own types")
                .0
        }

        /// Seals the match's unit types, once, with no effect for any tag but an engine tag's own,
        /// as `seal_types_with` does.
        pub(crate) fn seal_types(world: &mut World) {
            if world.contains_resource::<TestUnitType>() || world.contains_resource::<TagBook>() {
                return;
            }
            Units::seal_types_with(world, &BTreeMap::new());
        }

        /// Seals the match's unit types: loads the match's own type, and gives the tag book their
        /// tags and `effects`, by tag, as a match's books take them once its packages load. A test
        /// that put a tag book of its own sealed them itself, and its match has no type of its own.
        pub(crate) fn seal_types_with(
            world: &mut World,
            effects: &BTreeMap<DeclaredName, TagData>,
        ) {
            assert!(
                !world.contains_resource::<TestUnitType>(),
                "a match seals its unit types once"
            );
            let unit_type = Units::load_next_type(world, &UnitTypeData::default());
            world.insert_resource(TestUnitType(unit_type));
            let book = world.non_send::<View>().types_mut().tag_book(effects);
            world.insert_resource(book);
        }

        /// Gives unit `id` tags of no name that block `blocks`, in place of its own.
        pub(crate) fn set_blocks(world: &mut World, id: StableId, blocks: &[Block]) {
            world.insert_parts(id, UnitTags::blocking(blocks));
        }

        /// Gives unit `id` tags of no name that block `blocks` in `stage` of tick `tick`, as a modifier
        /// applied in that stage would.
        pub(crate) fn block_at(
            world: &mut World,
            id: StableId,
            tick: u64,
            stage: SimSet,
            blocks: &'static [Block],
        ) {
            let entity = world.unit_entity(id);
            let block = move |now: Res<'_, SimTick>, mut tags: Query<'_, '_, &mut UnitTags>| {
                if now.start() == Tick::new(tick) {
                    *tags.get_mut(entity).expect("a unit with tags") = UnitTags::blocking(blocks);
                }
            };
            world
                .resource_mut::<Schedules>()
                .add_systems(SimUpdate, block.in_set(stage));
        }

        /// `probe(ctx, of)` in `source`, run on the units as they are now.
        pub(crate) fn probe(
            world: &mut World,
            source: &str,
            of: StableId,
        ) -> Result<Dynamic, CallError> {
            let ctx = world.non_send::<Ctx>().clone();
            ctx.view().read(world);
            let unit = ctx.view().unit(of).expect("a unit of the match");
            let mut host = world.non_send_mut::<ScriptHost>();
            let script = host.compile(source).expect("a probe compiles");
            let mut budget = Budget::new(u64::MAX);
            host.call(&mut budget, script, "probe", (ctx, unit))
                .map_err(CallError::from_script)
        }

        /// `expression` evaluated with `ctx` and `of`, the unit `of`, on the units as they are now.
        pub(crate) fn read(
            world: &mut World,
            expression: &str,
            of: StableId,
        ) -> Result<Dynamic, CallError> {
            Units::probe(world, &format!("fn probe(ctx, of) {{ {expression} }}"), of)
        }

        /// Compiles `source`, and adds the hooks the host reads of it to the match's book, as a
        /// test match has no package load to read them.
        pub(crate) fn compile_hooked(
            world: &mut World,
            source: &str,
        ) -> Result<ScriptId, ScriptError> {
            let script = Units::compile(world, source)?;
            world.resource_scope(|world, mut scripts: Mut<'_, ScriptBook>| {
                assert_eq!(script.index(), scripts.len(), "the book holds every script");
                scripts.push(world.non_send::<ScriptHost>().functions(script));
            });
            Ok(script)
        }

        /// Names, for the scripts of a match with no mode, its `damage_kinds`, its `pools` and its
        /// players' `resources`, each by id, as a mode's books name them.
        pub(crate) fn name_kinds(
            world: &World,
            damage_kinds: &[&str],
            pools: &[&str],
            resources: &[&str],
        ) {
            let names = |names: &[&str]| -> Arc<[DeclaredName]> {
                names
                    .iter()
                    .map(|name| DeclaredName::new(name).expect("a test names a name"))
                    .collect()
            };
            let view = world.non_send::<View>();
            view.set_mode_names(&names(damage_kinds), names(resources));
            StatsColumn::share_pool_names(view, names(pools));
        }

        /// Declares every tag the match's packages name, in their order, after the engine's tags,
        /// before any type, modifier or filter names one, so the tags are numbered the same however
        /// the packages load.
        pub(crate) fn declare_tags<'a>(
            world: &mut World,
            names: impl IntoIterator<Item = &'a str>,
        ) {
            let view = world.non_send::<View>();
            let mut types = view.types_mut();
            for name in names {
                types.declare(name);
            }
        }

        /// Loads the unit type `name` of `scope`, with its core fields: its tags, its params and
        /// its state fields, which the match's book of them and the view then hold. A name is one
        /// type's only in its scope.
        pub(crate) fn load_type(
            world: &mut World,
            scope: TypeScope,
            name: &str,
            data: &UnitTypeData,
        ) -> UnitType {
            let view = world.non_send::<View>().clone();
            // A package's own scope is that package's; the mode's, the mode package's.
            let package = match scope {
                TypeScope::Mode => 0,
                TypeScope::Package(package) => package,
            };
            let unit_type = view.types_mut().load(scope, package, name, data);
            view.share_type_names();
            let states = view.types_mut().state_book();
            UnitsColumn::share(&view, states.clone());
            world.insert_resource(states);
            unit_type
        }

        /// Loads `data` as the next unit type of the mode, named for its place.
        pub(crate) fn load_next_type(world: &mut World, data: &UnitTypeData) -> UnitType {
            let name = format!("type {}", world.non_send::<View>().types_count());
            Units::load_type(world, TypeScope::Mode, &name, data)
        }

        pub(crate) fn tagged_type(
            world: &mut World,
            tags: &[&str],
            params: &[(&str, Scalar)],
        ) -> UnitType {
            let data = UnitTypeData::of(tags, params);
            Units::load_next_type(world, &data)
        }
    }

    /// The type of the units a test spawns with no type of its own, which sealing the match's unit
    /// types loads.
    #[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct TestUnitType(pub(crate) UnitType);
}

#[cfg(test)]
mod tests;
