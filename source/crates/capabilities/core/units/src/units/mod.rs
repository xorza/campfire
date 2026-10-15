use bevy_ecs::change_detection::CheckChangeTicks;
use bevy_ecs::observer::On;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::system::NonSend;
use bevy_ecs::world::World;
use campfire_declare::{
    ByType, Declarations, InstallCx, KindVisitor, Module, ModuleSet, Phase, SetPlace,
};
use campfire_script::{
    CoreApi, Ctx, ScriptBudgets, ScriptCall, ScriptError, ScriptFailures, ScriptHost, ScriptId,
    ScriptList, StateSchema, SyncTo, UnitRow, View, ViewApi,
};
use campfire_sim::{Breach, Capability, Either, Position, SimEdge, SimSet, TickRate};
use campfire_values::{EngineTag, Relations, TagProperties, Team, UnitTags, UnitType};

use crate::units::arrived_short::ArrivedShort;
use crate::units::avatar::Avatar;
use crate::units::body::Body;
use crate::units::body_form::BodyForm;
use crate::units::dead::Dead;
use crate::units::destination::Destination;
use crate::units::facing::Facing;
use crate::units::forced_move::ForcedMove;
use crate::units::lifespan::Lifespan;
use crate::units::map_book::MapBook;
use crate::units::match_entity::MatchEntity;
use crate::units::move_step::MoveStep;
use crate::units::new_unit::NewUnit;
use crate::units::new_unit_states::NewUnitStates;
use crate::units::owner::Owner;
use crate::units::player::Player;
use crate::units::script_book::ScriptBook;
use crate::units::spawn_hooks::SpawnHooks;
use crate::units::spawn_point::SpawnPoint;
use crate::units::status_tags::StatusTags;
use crate::units::tag_book::TagBook;
use crate::units::turn_step::TurnStep;
use crate::units::type_origin_book::TypeOriginBook;
use crate::units::unit_entity::UnitEntity;
use crate::units::unit_entity::body_unit::BodyUnit;
use crate::units::unit_entity::delivery_entity::DeliveryEntity;
use crate::units::unit_grid::{GridUnits, UnitGrid};
use crate::units::unit_state::{SyncAll, SyncNone, SyncOwner, SyncTeam, UnitState};
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_ticks::UnitTicks;
use crate::units::unit_tracker::UnitTracker;
use crate::units::unit_type_book::UnitTypeBook;
use crate::units::units_api::UnitsApi;
use crate::units::units_call::UnitsCall;
use crate::units::units_column::{StateRowReads, UnitsColumn};
use crate::units::view_reader::ViewReader;

pub(crate) mod arrived_short;
pub(crate) mod avatar;
pub(crate) mod body;
pub(crate) mod body_form;
pub(crate) mod collision_data;
pub(crate) mod dead;
pub(crate) mod destination;
pub(crate) mod entity_filters;
pub(crate) mod entity_kinds;
pub(crate) mod error;
pub(crate) mod facing;
pub(crate) mod forced_move;
pub(crate) mod hit_handle;
pub(crate) mod layer;
pub(crate) mod lifespan;
pub(crate) mod map_book;
pub(crate) mod match_entity;
pub(crate) mod move_step;
pub(crate) mod new_unit;
pub(crate) mod new_unit_states;
pub(crate) mod owner;
pub(crate) mod player;
pub(crate) mod player_units;
pub(crate) mod row_marks;
pub(crate) mod row_source;
pub(crate) mod script_book;

pub(crate) mod source_reads;
pub(crate) mod spawn_at;
pub(crate) mod spawn_hooks;
pub(crate) mod spawn_point;
pub(crate) mod spawn_target;
pub(crate) mod spawner;
pub(crate) mod state_write;
pub(crate) mod status_tags;
pub(crate) mod tag_book;
pub(crate) mod tag_data;
pub(crate) mod turn_step;
pub(crate) mod type_origin_book;
pub(crate) mod unit_core;
pub(crate) mod unit_entity;
pub(crate) mod unit_grid;
pub(crate) mod unit_state;
pub(crate) mod unit_state_access;
pub(crate) mod unit_state_book;
pub(crate) mod unit_states;
pub(crate) mod unit_ticks;
pub(crate) mod unit_tracker;
pub(crate) mod unit_type_book;
pub(crate) mod unit_type_core;
pub(crate) mod units_api;
pub(crate) mod units_call;
pub(crate) mod units_column;
pub(crate) mod units_world;
pub(crate) mod view_reader;

campfire_common::enum_all! {
    /// The core's steps.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum UnitsSet {
        /// Every pool starts full.
        BeginTick,
        /// The units whose lifespan ended despawn.
        EndLifespans,
        /// The script states of units that never spawned are dropped.
        DropUnspawned,
    }
    /// Every step.
    pub const ALL;
}

impl ModuleSet for UnitsSet {
    const CAPABILITY: Option<Capability> = None;
    const ALL: &'static [UnitsSet] = &UnitsSet::ALL;

    fn place(self) -> SetPlace {
        match self {
            UnitsSet::BeginTick => SetPlace::Phase(Phase::Begin),
            UnitsSet::EndLifespans => SetPlace::Phase(Phase::End),
            UnitsSet::DropUnspawned => SetPlace::Gap(SimEdge::After(SimSet::Vision)),
        }
    }
}

/// The core under every capability's scripts: unit types, the script host they run in, and the
/// units as scripts see them, with the queries on them. Every match installs it before its
/// capabilities.
#[derive(Debug)]
pub struct Units;

impl Module for Units {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<UnitsSet>();
        d.calls::<SpawnHooks>();
        d.engine_tags(&[Units::AVATAR, Dead::TAG]);
        Units::kinds(d);
        d.component::<Body>();
        d.component::<BodyUnit>();
        d.component::<Dead>();
        d.component::<Destination>();
        d.component::<Facing>();
        d.component::<ForcedMove>();
        d.component::<Lifespan>();
        d.component::<MoveStep>();
        d.component::<TurnStep>();
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
        d.derived::<Avatar>();
        d.derived::<ArrivedShort>();
        d.component::<Relations>();
        d.scratch::<NewUnitStates>();
        d.cache::<UnitGrid>();
        d.cache::<UnitTracker<GridUnits>>();
        d.observer(UnitTracker::<GridUnits>::aged);
        d.observer(UnitTracker::<GridUnits>::lost_body);
        d.observer(Units::refill_view);
        d.check::<Position>(Units::within_bounds);
        d.host::<Position, UnitEntity>();
        d.check::<Team>(Units::teams_known);
        d.host::<Team, Either<UnitEntity, Player>>();
        d.check::<UnitType>(Units::types_known);
        d.host::<UnitType, UnitEntity>();
        d.check::<Relations>(Units::relations_known);
        d.host::<Relations, MatchEntity>();
        d.require::<UnitEntity, Position>();
        d.require::<UnitEntity, Team>();
        // A forced move moves a unit that walks, in place of its step.
        d.require::<ForcedMove, MoveStep>();
        d.book::<UnitTypeBook>();
        d.book::<UnitStateBook>();
        d.book::<TypeOriginBook>();
        d.book::<TagBook>();
        d.book::<MapBook>();
        d.book::<ScriptBook>();
        d.column::<UnitsColumn, StateRowReads>(UnitsColumn::fill_state);
        d.call_part::<UnitsCall>();
        d.call_part::<ScriptCall>();
        d.api(CoreApi::register);
        d.api(ViewApi::register_unit);
        d.api(NewUnit::register);
        d.api(UnitsApi::register);
        d.api(ViewApi::register_queries);
        d.book::<ByType<MoveStep>>();
        d.book::<ByType<TurnStep>>();
        d.book::<ByType<BodyForm>>();
        // The spawner gives a unit the facing of its angle, which its body turns by; this part
        // holds every unit to one.
        d.part::<BodyUnit, Facing>(|at| Some(Facing::of(Facing::angle_of(at))));
        d.part::<BodyUnit, SpawnPoint>(|at| at.pos().map(SpawnPoint::new));
        d.part::<BodyUnit, Body>(|at| {
            at.of_type::<BodyForm>()
                .map(|form| form.at(Facing::angle_of(at)))
        });
        d.part::<BodyUnit, MoveStep>(|at| at.of_type::<MoveStep>().copied());
        d.part::<BodyUnit, TurnStep>(|at| at.of_type::<TurnStep>().copied());
        // A unit that walks, which its move step tells, takes a destination.
        d.part::<BodyUnit, Destination>(|at| {
            at.of_type::<MoveStep>().map(|_| Destination::default())
        });
    }

    /// Adds the core to a match: the grid of its body units, empty, which its readers build. With
    /// scripts, in Inputs, every pool starts full and the last tick's failures clear, and as the
    /// tick ends, its writes to units that never spawned drop.
    fn install(cx: &mut InstallCx<'_>) {
        let world = &mut *cx.world;
        world
            .register_component_hooks::<UnitType>()
            .on_insert(TagBook::give_tags);
        world.init_resource::<UnitGrid>();
        world.init_resource::<UnitTracker<GridUnits>>();
        cx.scripted((
            UnitTicks::begin_tick.in_set(UnitsSet::BeginTick),
            UnitTicks::end_lifespans.in_set(UnitsSet::EndLifespans),
            UnitTicks::drop_unspawned_states.in_set(UnitsSet::DropUnspawned),
        ));
    }
}

impl Units {
    /// The engine tag of avatars, the units players play as, which `unit.is_avatar` tests.
    pub const AVATAR: EngineTag = EngineTag::new(0, "avatar", TagProperties::NONE);

    /// Whether the unit of `row` is an avatar.
    pub(crate) const fn is_avatar(row: &UnitRow) -> bool {
        row.tags.tags.contains(Units::AVATAR.tag())
    }

    /// Every avatar of `view`, living or dead, of `team` or of every team, by stable id.
    pub fn avatars(view: &View, team: Option<Team>) -> ScriptList {
        view.units_where(|row| Units::is_avatar(row) && team.is_none_or(|team| row.team == team))
    }

    /// The observer of Bevy's check of change ticks: the script view's next read fills every
    /// row, as its queries of changed parts may miss a change after the check.
    fn refill_view(_: On<'_, '_, CheckChangeTicks>, view: NonSend<'_, View>) {
        view.refill_next();
    }

    /// Visits each kind of entity of the core's units, which state types name as their host.
    fn kinds<V: KindVisitor>(v: &mut V) {
        v.kind::<BodyUnit>();
        v.kind::<DeliveryEntity>();
    }

    /// Whether every unit stands within the match's bounds, as every system keeps it, and as
    /// vision and navigation index the map's cells by it; the first that does not.
    /// A team the mode lacks has no place in the vision groups and the mode's tables.
    fn teams_known(world: &World) -> Result<(), Breach> {
        let names = world.non_send::<View>().names();
        Breach::first(world, |entity| {
            world
                .get::<Team>(entity)
                .is_some_and(|&team| !names.has_team(team))
        })
    }

    /// A type the match did not load names nothing its books hold, and a unit without a class of
    /// the state its type declares would read a field it lacks.
    fn types_known(world: &World) -> Result<(), Breach> {
        let types = world.resource::<UnitTypeBook>();
        let book = world.resource::<UnitStateBook>();
        Breach::first(world, |entity| {
            let Some(&unit_type) = world.get::<UnitType>(entity) else {
                return false;
            };
            let unit = world.entity(entity);
            let states = |schema: &StateSchema| {
                schema.places().iter().all(|place| match place.sync {
                    SyncTo::None => unit.contains::<UnitState<SyncNone>>(),
                    SyncTo::Owner => unit.contains::<UnitState<SyncOwner>>(),
                    SyncTo::Team => unit.contains::<UnitState<SyncTeam>>(),
                    SyncTo::All => unit.contains::<UnitState<SyncAll>>(),
                })
            };
            let schema = book.schema(unit_type);
            !(types.contains(unit_type) && schema.is_some_and(states))
        })
    }

    /// A relation of a team the mode lacks would join it to a vision group of the mode's teams.
    fn relations_known(world: &World) -> Result<(), Breach> {
        let names = world.non_send::<View>().names();
        Breach::first(world, |entity| {
            world
                .get::<Relations>(entity)
                .is_some_and(|relations| !relations.teams().all(|team| names.has_team(team)))
        })
    }

    fn within_bounds(world: &World) -> Result<(), Breach> {
        let bounds = world.resource::<MapBook>().bounds;
        Breach::first(world, |entity| {
            world
                .get::<Position>(entity)
                .is_some_and(|&at| !bounds.contains(at))
        })
    }

    /// Compiles `source` in the match's script host, once for every capability that runs it.
    pub fn compile(world: &mut World, source: &str) -> Result<ScriptId, ScriptError> {
        world.non_send_mut::<ScriptHost>().compile(source)
    }

    /// Opens a match for the modules' installs: the script view and its reader, and with
    /// `budgets`, the script host the scripts run in within them, and the context of their calls.
    /// A client runs no scripts.
    pub fn open(world: &mut World, budgets: Option<ScriptBudgets>) {
        let rate = *world.resource::<TickRate>();
        let view = View::new(rate, ViewReader::read);
        world.insert_non_send(ViewReader::default());
        if let Some(budgets) = budgets {
            let ctx = Ctx::new(view.clone());
            let mut host = ScriptHost::new(budgets.limits().per_call);
            ctx.tag(&mut host);
            world.insert_non_send(ctx);
            world.insert_non_send(host);
            world.insert_non_send(ScriptFailures::default());
            world.insert_resource(budgets);
        }
        world.insert_non_send(view);
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::change_detection::Mut;
    use bevy_ecs::world::World;
    use campfire_declare::{Book, ByType};
    use campfire_script::{CallError, Ctx, ScriptError, ScriptHost, ScriptId, ScriptValue, View};
    use campfire_sim::StableId;
    use campfire_values::{Block, DeclaredName, Relation, Scalar, Team, TypeScope, UnitType};

    use crate::units::Units;
    use crate::units::script_book::ScriptBook;
    use crate::units::tag_book::TagBook;
    use crate::units::tag_data::TagData;
    use crate::units::unit_type_book::UnitTypeBook;
    use crate::units::unit_type_core::UnitTypeCore;
    use crate::units::units_world::UnitsWorldCrate;

    impl Units {
        /// The unit type named `name` in the mode's scope: one of the mode's, or an avatar.
        pub fn type_named(world: &World, name: &str) -> Option<UnitType> {
            world
                .non_send::<View>()
                .names()
                .named((TypeScope::Mode, name))
        }

        /// Sets how the two teams of the match regard each other, as `ctx.set_relation`
        /// does, their vision as it was.
        pub fn set_relation(world: &mut World, of: Team, other: Team, relation: Relation) {
            world
                .relations_mut()
                .set_keeping_vision(of, other, relation);
        }
    }

    /// The package of the modifiers that grant the test tags, after every package a test
    /// loads its own from.
    pub const TEST_PACKAGE: u16 = u16::MAX;

    /// How many test tags `Units::test_tags` declares.
    const TEST_TAGS: usize = Block::ALL.len() + 2;

    impl Units {
        /// Seals the match's unit types' tags: gives the tag book their tags and `effects`, by
        /// tag, as a match's books take them once its packages load, and the test tags, which
        /// it declares after every type's, with theirs.
        pub fn seal_tags(world: &mut World, effects: &BTreeMap<DeclaredName, TagData>) {
            let mut effects = effects.clone();
            let mut types = world.resource_mut::<UnitTypeBook>();
            for (name, data) in Units::test_tags() {
                types.declare(name.as_str()).unwrap();
                effects.insert(name, data);
            }
            let book = types.tag_book(&effects);
            world.insert_resource(book);
            UnitTypeBook::share_in(world);
        }

        /// The tags a test holds on a unit, each by a modifier of its name that grants it, with
        /// its effect: one that blocks each block, in the order of `Block::ALL`, then one hidden,
        /// then one that detects.
        pub fn test_tags() -> [(DeclaredName, TagData); TEST_TAGS] {
            let name = |name: &str| DeclaredName::new(&format!("test_{name}")).unwrap();
            let blocks = Block::ALL.into_iter().map(|block| {
                let data = TagData {
                    blocks: vec![block],
                    ..TagData::default()
                };
                (name(&format!("blocks_{}", block.name())), data)
            });
            let hidden = TagData {
                hidden: true,
                ..TagData::default()
            };
            let detects = TagData {
                detects: true,
                ..TagData::default()
            };
            let all: Vec<_> = blocks
                .chain([(name("hidden"), hidden), (name("detects"), detects)])
                .collect();
            all.try_into().expect("a test tag for each block, then two")
        }

        /// Seals the match's unit types' tags as `seal_tags` does, with no effect for a tag,
        /// unless they are sealed, or the test put a tag book of its own.
        pub fn seal_once(world: &mut World) {
            let sealed = world.resource::<UnitTypeBook>().sealed();
            if !sealed && world.resource::<TagBook>().is_empty() {
                Units::seal_tags(world, &BTreeMap::new());
            }
        }

        /// Gives `unit_type` `value` in its book, as the books of a mode's type do.
        pub fn give_type<T: Send + Sync + 'static>(
            world: &mut World,
            unit_type: UnitType,
            value: T,
        ) {
            world.resource_mut::<ByType<T>>().set(unit_type, value);
        }

        /// `probe(ctx, of)` in `source`, run on the units as they are now.
        pub fn probe(
            world: &mut World,
            source: &str,
            of: StableId,
        ) -> Result<ScriptValue, CallError> {
            let ctx = world.non_send::<Ctx>().clone();
            ctx.view().read(world);
            let unit = ctx.view().unit(of).expect("a unit of the match");
            ScriptValue::probe(world, source, (ctx, unit))
        }

        /// `expression` evaluated with `ctx` and `of`, the unit `of`, on the units as they are now.
        pub fn read(
            world: &mut World,
            expression: &str,
            of: StableId,
        ) -> Result<ScriptValue, CallError> {
            Units::probe(world, &format!("fn probe(ctx, of) {{ {expression} }}"), of)
        }

        /// Compiles `source`, and adds the hooks the host reads of it to the match's book, as a
        /// test match has no package load to read them.
        pub fn compile_hooked(world: &mut World, source: &str) -> Result<ScriptId, ScriptError> {
            let script = Units::compile(world, source)?;
            world.resource_scope(|world, mut scripts: Mut<'_, ScriptBook>| {
                assert_eq!(script.index(), scripts.len(), "the book holds every script");
                scripts.push(world.non_send::<ScriptHost>().functions(script));
            });
            Ok(script)
        }

        /// Declares every tag the match's packages name, in their order, after the engine's tags,
        /// before any type, modifier or filter names one, so the tags are numbered the same however
        /// the packages load.
        pub fn declare_tags<'a>(world: &mut World, names: impl IntoIterator<Item = &'a str>) {
            let mut types = world.resource_mut::<UnitTypeBook>();
            for name in names {
                types.declare(name).unwrap();
            }
            UnitTypeBook::share_in(world);
        }

        /// Loads the unit type `name` of `scope`, with its core fields: its tags, its params and
        /// its state fields, which the match's book of them and the view then hold. A name is one
        /// type's only in its scope.
        pub fn load_type(
            world: &mut World,
            scope: TypeScope,
            name: &str,
            data: &UnitTypeCore,
        ) -> UnitType {
            // A package's own scope is that package's; the mode's, the mode package's.
            let package = match scope {
                TypeScope::Mode => 0,
                TypeScope::Package(package) => package,
            };
            let mut types = world.resource_mut::<UnitTypeBook>();
            let unit_type = types.load(scope, package, name, data).unwrap();
            let states = types.state_book();
            UnitTypeBook::share_in(world);
            states.load_into(world);
            unit_type
        }

        /// Loads `data` as the next unit type of the mode, named for its place.
        pub fn load_next_type(world: &mut World, data: &UnitTypeCore) -> UnitType {
            let name = format!("type {}", world.resource::<UnitTypeBook>().count());
            Units::load_type(world, TypeScope::Mode, &name, data)
        }

        pub fn tagged_type(
            world: &mut World,
            tags: &[&str],
            params: &[(&str, Scalar)],
        ) -> UnitType {
            let data = UnitTypeCore::of(tags, params);
            Units::load_next_type(world, &data)
        }
    }
}
