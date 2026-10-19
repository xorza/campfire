use bevy_ecs::change_detection::CheckChangeTicks;
use bevy_ecs::entity::Entity;
use bevy_ecs::observer::On;
use bevy_ecs::schedule::SystemSet;
use bevy_ecs::system::NonSend;
use bevy_ecs::world::World;
use campfire_common::EnumAll;
use campfire_declare::{
    ByType, Declarations, InstallCx, KindVisitor, Module, ModuleSet, SetPlace, StateFields,
};
use campfire_script::{
    CoreApi, Ctx, Listing, MatchNames, ScriptBudgets, ScriptCall, ScriptError, ScriptFailures,
    ScriptHost, ScriptId, ScriptList, ScriptPrints, StatePlace, StateSchema, View, ViewApi,
};
use campfire_sim::{Breach, Capability, MatchRate, Position, SimEdge, SimSet};
use campfire_values::{Relations, Team, UnitTags, UnitType};

use crate::arrived_short::ArrivedShort;
use crate::avatar::Avatar;
use crate::body::Body;
use crate::body_form::BodyForm;
use crate::collision_data::CollisionData;
use crate::dead::Dead;
use crate::declared_tags::DeclaredTags;
use crate::destination::Destination;
use crate::facing::Facing;
use crate::forced_move::ForcedMove;
use crate::ground_data::GroundData;
use crate::lifespan::Lifespan;
use crate::map_book::{BoundsData, MapBook, MetricData};
use crate::move_step::MoveStep;
use crate::new_unit_states::NewUnitStates;
use crate::owner::Owner;
use crate::script_book::ScriptBook;
use crate::spawn_callbacks::SpawnCallbacks;
use crate::spawn_point::SpawnPoint;
use crate::status_tags::StatusTags;
use crate::tag_book::TagBook;
use crate::tag_data::ModeTagsData;
use crate::turn_step::TurnStep;
use crate::type_origin_book::TypeOriginBook;
use crate::unit_entity::UnitEntity;
use crate::unit_entity::body_unit::BodyUnit;
use crate::unit_entity::delivery_entity::DeliveryEntity;
use crate::unit_grid::{GridUnits, UnitGrid};
use crate::unit_state::{StateSync, SyncAll, SyncCall, SyncNone, SyncOwner, SyncTeam, UnitState};
use crate::unit_state_book::UnitStateBook;
use crate::unit_steps::UnitSteps;
use crate::unit_ticks::UnitTicks;
use crate::unit_tracker::UnitTracker;
use crate::unit_type_book::UnitTypeBook;
use crate::unit_type_core::{TypeParamsData, TypeStateData, TypeTagsData};
use crate::unit_types::UnitTypes;
use crate::units_api::UnitsApi;
use crate::units_call::UnitsCall;
use crate::units_column::{StateRowReads, UnitsColumn};
use crate::view_reader::ViewReader;

/// The core's steps, the first and the last of each tick, which every module's start and end of
/// the tick sit at.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum UnitsSet {
    /// The tick begins: every pool starts full, and the last tick's failures clear.
    Begin,
    /// The tick ends: the units whose lifespan ended despawn.
    End,
    /// The tick closes: the script states of units that never spawned are dropped, and what the
    /// scripts printed reaches its report.
    Close,
}

impl ModuleSet for UnitsSet {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Begin => SetPlace::stage(SimSet::Inputs),
            Self::End => SetPlace::stage(SimSet::Vision),
            Self::Close => SetPlace::gap(SimEdge::After(SimSet::Vision)),
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
    type Set = UnitsSet;

    fn declare<D: Declarations>(d: &mut D) {
        // Before every book, which shares its names into it as it installs.
        d.cache::<MatchNames>();
        Self::runtime(d);
        d.calls::<SpawnCallbacks>();
        d.engine_tags(&[Avatar::TAG, Dead::TAG]);
        d.section::<TypeTagsData>();
        d.section::<TypeParamsData>();
        d.section::<TypeStateData>();
        d.extend::<StateFields>(Self::state_fields);
        d.section::<CollisionData>();
        d.section::<ModeTagsData>();
        d.section::<MetricData>();
        d.section::<BoundsData>();
        d.section::<GroundData>();
        Self::kinds(d);
        d.component::<Body>();
        d.component::<BodyUnit>();
        d.component::<Dead>();
        d.component::<Destination>();
        d.component::<Facing>();
        d.component::<ForcedMove>();
        d.component::<Lifespan>();
        d.timed::<Lifespan>();
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
        d.report::<ScriptBudgets>();
        d.report::<ScriptPrints>();
        d.report::<ScriptFailures>();
        d.on_insert::<UnitType>(TagBook::give_tags);
        d.cache::<UnitGrid>();
        d.cache::<UnitTracker<GridUnits>>();
        d.observer(UnitTracker::<GridUnits>::aged);
        d.observer(UnitTracker::<GridUnits>::lost_body);
        d.observer(Self::refill_view);
        d.check::<Position>(Self::within_bounds);
        d.host::<Position, UnitEntity>();
        d.check::<Team>(Self::teams_known);
        d.check::<UnitType>(Self::types_known);
        d.host::<UnitType, UnitEntity>();
        d.check::<Relations>(Self::relations_known);
        d.require::<UnitEntity, Position>();
        d.require::<UnitEntity, Team>();
        // A forced move moves a unit that walks, in place of its step.
        d.require::<ForcedMove, MoveStep>();
        d.fact::<DeclaredTags>(DeclaredTags::build);
        d.fact::<UnitTypes>(UnitTypes::build);
        d.extend::<UnitTypes>(UnitTypes::tag_bodies);
        d.book::<UnitTypeBook>(UnitTypes::build_type_book);
        d.book::<UnitStateBook>(UnitTypes::build_state_book);
        d.book::<TypeOriginBook>(UnitTypes::build_origin_book);
        d.book::<TagBook>(UnitTypes::build_tag_book);
        d.rule(Self::rule);
        d.book::<MapBook>(MapBook::build);
        d.book::<ScriptBook>(ScriptBook::build);
        d.column::<UnitsColumn, StateRowReads>(UnitsColumn::fill_state);
        d.call_extension::<UnitsCall>();
        d.call_extension::<ScriptCall>();
        d.api(CoreApi::register);
        d.api(ViewApi::register_unit);
        d.api(UnitsApi::register);
        d.api(ViewApi::register_queries);
        d.book::<ByType<MoveStep>>(UnitSteps::steps);
        d.book::<ByType<TurnStep>>(UnitSteps::turns);
        d.book::<ByType<BodyForm>>(CollisionData::bodies);
        // A unit faces its spawn's angle, which its body turns by.
        d.part::<BodyUnit, Facing>(|at| Some(Facing::of(Facing::angle_of(at))));
        d.part::<BodyUnit, SpawnPoint>(|at| at.position().map(SpawnPoint::new));
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
    fn install(cx: &mut InstallCx<'_, Self::Set>) {
        cx.scripted(UnitsSet::Begin, UnitTicks::begin_tick);
        cx.scripted(UnitsSet::End, UnitTicks::end_lifespans);
        cx.scripted(
            UnitsSet::Close,
            (UnitTicks::drop_unspawned_states, UnitTicks::report_prints),
        );
    }
}

impl Units {
    /// Every avatar of `view`, living or dead, of `team` or of every team, by stable id.
    pub fn avatars(view: &View, team: Option<Team>, listing: Listing) -> ScriptList {
        view.units_where(|row| {
            listing.lists(row) && Avatar::is_tagged(row) && team.is_none_or(|team| row.team == team)
        })
    }

    /// The observer of Bevy's check of change ticks: the script view's next read fills every
    /// row, as its queries of changed parts may miss a change after the check.
    fn refill_view(_: On<'_, '_, CheckChangeTicks>, view: NonSend<'_, View>) {
        view.refill_next();
    }

    /// Visits each kind of entity of the core's units, which state types name as their host.
    fn kinds<V: KindVisitor>(v: &mut V) {
        v.kind::<BodyUnit>();
        v.state::<BodyUnit, Dead>();
        v.state::<BodyUnit, ForcedMove>();
        v.kind::<DeliveryEntity>();
    }

    /// Whether every unit's team is one the mode has, as a team the mode lacks has no place in
    /// the vision groups and the mode's tables; the first that is not.
    fn teams_known(world: &World) -> Result<(), Breach> {
        let names = world.resource::<MatchNames>();
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
            let states = |schema: &StateSchema| {
                let holds = |place: &StatePlace| HoldsClass { world, entity }.on(place.sync);
                schema.places().iter().all(holds)
            };
            let schema = book.schema(unit_type);
            !(types.contains(unit_type) && schema.is_some_and(states))
        })
    }

    /// A relation of a team the mode lacks would join it to a vision group of the mode's teams.
    fn relations_known(world: &World) -> Result<(), Breach> {
        let names = world.resource::<MatchNames>();
        Breach::first(world, |entity| {
            world
                .get::<Relations>(entity)
                .is_some_and(|relations| !relations.teams().all(|team| names.has_team(team)))
        })
    }

    /// Whether every unit stands within the match's bounds, as every system keeps it, and as
    /// vision and navigation index the map's cells by it; the first that does not.
    fn within_bounds(world: &World) -> Result<(), Breach> {
        let bounds = world.resource::<MapBook>().bounds;
        Breach::first(world, |entity| {
            world
                .get::<Position>(entity)
                .is_some_and(|at| !bounds.contains(at.get()))
        })
    }

    /// Compiles `source` in the match's script host, once for every capability that runs it.
    pub fn compile(world: &mut World, source: &str) -> Result<ScriptId, ScriptError> {
        world.non_send_mut::<ScriptHost>().compile(source)
    }

    /// The scripts' runtime, which the core's install puts in first, for the modules' installs:
    /// the script view and its reader, and in a match that runs scripts, the context of their
    /// calls, the script host they run in within the role's budgets, and their failures. A client
    /// runs no scripts.
    fn runtime<D: Declarations>(d: &mut D) {
        d.cache::<ViewReader>();
        d.non_send::<View>(|world, _| {
            Some(View::new(*world.resource::<MatchRate>(), ViewReader::read))
        });
        d.non_send::<Ctx>(|world, role| {
            role.scripts()
                .then(|| Ctx::new(world.non_send::<View>().clone()))
        });
        d.non_send::<ScriptHost>(|world, role| {
            let pools = role.pools()?;
            let mut host = ScriptHost::new(pools.limits.per_call);
            world.non_send::<Ctx>().tag(&mut host);
            Some(host)
        });
    }
}

/// Whether the unit of `entity` holds a class's `UnitState`.
#[derive(Debug)]
struct HoldsClass<'w> {
    world: &'w World,
    entity: Entity,
}

impl SyncCall for HoldsClass<'_> {
    type Output = bool;

    fn call<S: StateSync>(self) -> bool {
        self.world.entity(self.entity).contains::<UnitState<S>>()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::change_detection::Mut;
    use bevy_ecs::world::World;
    use campfire_common::{DeclaredName, NamedEnum};
    use campfire_declare::ByType;
    use campfire_declare::internals::BookLoad;
    use campfire_script::{
        CallError, Ctx, MatchNames, ScriptError, ScriptHost, ScriptId, ScriptValue,
    };
    use campfire_sim::StableId;
    use campfire_values::{Block, PackageIndex, Relation, Scalar, Team, TypeScope, UnitType};

    use crate::script_book::ScriptBook;
    use crate::tag_book::TagBook;
    use crate::tag_data::TagData;
    use crate::type_origin_book::TypeOrigin;
    use crate::unit_type_book::UnitTypeBook;
    use crate::unit_type_core::UnitTypeCore;
    use crate::unit_types::UnitTypes;
    use crate::units::Units;
    use crate::units_world::UnitsWorld;

    impl Units {
        /// The unit type named `name` in the mode's scope: one of the mode's, or an avatar.
        pub fn type_named(world: &World, name: &str) -> Option<UnitType> {
            world
                .resource::<MatchNames>()
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
    pub const TEST_PACKAGE: PackageIndex = PackageIndex::new(u16::MAX);

    /// How many test tags `Units::test_tags` declares.
    const TEST_TAGS: usize = Block::ALL.len() + 2;

    impl Units {
        /// `change` of the unit types the match loads, after which the match's book of their
        /// names and of their state fields holds them, as a test match has no package load.
        pub fn change_types<R>(
            world: &mut World,
            change: impl FnOnce(&mut World, &mut UnitTypes) -> R,
        ) -> R {
            let mut types = world.resource_mut::<UnitTypeBook>().take_loading();
            let changed = change(world, &mut types);
            let states = types.state_book();
            types.into_book().load_into(world);
            states.load_into(world);
            changed
        }

        /// Finishes the match's unit types, which gives it their books, their tags with the
        /// effects `data` gives them by name, as a match's books take them once its packages
        /// load: after it, no type loads.
        pub fn seal_types(world: &mut World, data: &BTreeMap<DeclaredName, TagData>) {
            let types = world.resource_mut::<UnitTypeBook>().take_loading();
            let books = types.finish(data);
            books.types.load_into(world);
            books.tags.load_into(world);
            books.origins.load_into(world);
            books.states.load_into(world);
        }

        /// Seals the match's unit types' tags as `seal_types` does, with `effects`, and the test
        /// tags, which it declares after every type's, with theirs.
        pub fn seal_tags(world: &mut World, effects: &BTreeMap<DeclaredName, TagData>) {
            let mut effects = effects.clone();
            Self::change_types(world, |_, types| {
                for (name, data) in Self::test_tags() {
                    types.declare(name.as_str()).unwrap();
                    effects.insert(name, data);
                }
            });
            Self::seal_types(world, &effects);
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
            let loading = world.resource::<UnitTypeBook>().is_loading();
            if loading && world.resource::<TagBook>().is_empty() {
                Self::seal_tags(world, &BTreeMap::new());
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
            Self::probe(world, &format!("fn probe(ctx, of) {{ {expression} }}"), of)
        }

        /// Compiles `source`, and adds the hooks the host reads of it to the match's book, as a
        /// test match has no package load to read them.
        pub fn compile_hooked(world: &mut World, source: &str) -> Result<ScriptId, ScriptError> {
            let script = Self::compile(world, source)?;
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
            Self::change_types(world, |_, types| {
                for name in names {
                    types.declare(name).unwrap();
                }
            });
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
                TypeScope::Mode => PackageIndex::MODE,
                TypeScope::Package(package) => package,
            };
            Self::change_types(world, |_, types| {
                let origin = TypeOrigin {
                    package,
                    name: name.into(),
                };
                types.load(scope, origin, data).unwrap()
            })
        }

        /// Loads `data` as the next unit type of the mode, named for its place.
        pub fn load_next_type(world: &mut World, data: &UnitTypeCore) -> UnitType {
            let name = format!("type {}", world.resource::<UnitTypeBook>().count());
            Self::load_type(world, TypeScope::Mode, &name, data)
        }

        pub fn tagged_type(
            world: &mut World,
            tags: &[&str],
            params: &[(&str, Scalar)],
        ) -> UnitType {
            let data = UnitTypeCore::of(tags, params);
            Self::load_next_type(world, &data)
        }
    }
}
