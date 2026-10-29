use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::SystemSet;
use bevy_ecs::world::World;
use campfire_common::EnumAll;
use campfire_declarations::{
    BookCx, BookError, ById, ByIdBuilder, Declarations, InstallCx, KindVisitor, Module,
    ModuleSystems, SetPlace, StateFields,
};
use campfire_names::MatchNames;
use campfire_script::{Listing, ScriptCall, ScriptList, StatePlace, StateSchema, View};
use campfire_sim::{Breach, Capability, Position, SimEdgeSystems, SimSystems};
use campfire_values::{Relations, Tag, Team, UnitTags, UnitType};

use crate::accel_step::AccelStep;
use crate::arrived_short::ArrivedShort;
use crate::avatar::Avatar;
use crate::body::Body;
use crate::body_form::BodyForm;
use crate::collision_data::{CollisionData, VariationsData};
use crate::dead::Dead;
use crate::declared_tags::DeclaredTags;
use crate::destination::Destination;
use crate::facing::Facing;
use crate::firing_rider::FiringRider;
use crate::forced_move::ForcedMove;
use crate::ground_data::GroundData;
use crate::leaving::Leaving;
use crate::lifespan::Lifespan;
use crate::map_book::{BoundsData, MapBook, MetricData};
use crate::move_step::MoveStep;
use crate::new_unit_states::NewUnitStates;
use crate::owner::Owner;
use crate::script_book::ScriptBook;
use crate::seeing_rider::SeeingRider;
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
use crate::unit_state::{
    AudienceAll, AudienceCall, AudienceNone, AudienceOwner, AudienceTeam, StateAudience, UnitState,
};
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
use crate::variation::Variation;
use crate::variations::Variations;

/// The core's steps, the first and the last of each tick, which every module's start and end of
/// the tick sit at.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum UnitsSystems {
    /// The tick begins, before any input: the steps that start each tick, the scripts' among
    /// them.
    Begin,
    /// The tick ends: the units whose lifespan ended despawn.
    End,
    /// The tick closes: the script states of units that never spawned are dropped, and what the
    /// scripts printed reaches its report.
    Close,
}

impl ModuleSystems for UnitsSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Begin => SetPlace::stage(SimSystems::Inputs),
            Self::End => SetPlace::stage(SimSystems::Vision),
            Self::Close => SetPlace::gap(SimEdgeSystems::After(SimSystems::Vision)),
        }
    }
}

/// The core under every capability: unit types, their bodies, states and tags, the map, and the
/// units' script books. Every match installs it before its capabilities, after the scripts'
/// runtime, `Scripts`.
#[derive(Debug)]
pub struct Units;

impl Module for Units {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[];
    type Systems = UnitsSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.calls::<SpawnCallbacks>();
        d.engine_tags(&[Avatar::TAG, Dead::TAG]);
        d.section::<TypeTagsData>();
        d.section::<TypeParamsData>();
        d.section::<TypeStateData>();
        d.extend::<StateFields>(Self::state_fields);
        d.section::<CollisionData>();
        d.section::<VariationsData>();
        d.section::<ModeTagsData>();
        d.section::<MetricData>();
        d.section::<BoundsData>();
        d.section::<GroundData>();
        Self::kinds(d);
        d.component::<Body>();
        d.component::<Variation>();
        d.component::<BodyUnit>();
        d.component::<Dead>();
        d.component::<Destination>();
        d.component::<Leaving>();
        d.component::<FiringRider>();
        d.state::<BodyUnit, FiringRider>();
        d.component::<SeeingRider>();
        d.state::<BodyUnit, SeeingRider>();
        d.component::<Facing>();
        d.component::<ForcedMove>();
        d.component::<Lifespan>();
        d.timed::<Lifespan>();
        d.component::<MoveStep>();
        d.component::<TurnStep>();
        d.component::<AccelStep>();
        d.sim_component::<Position>();
        d.component::<Owner>();
        d.component::<SpawnPoint>();
        d.component::<StatusTags>();
        d.component::<Team>();
        d.component::<UnitEntity>();
        d.component::<UnitType>();
        d.component::<UnitState<AudienceNone>>();
        d.component::<UnitState<AudienceOwner>>();
        d.component::<UnitState<AudienceTeam>>();
        d.component::<UnitState<AudienceAll>>();
        d.derived::<DeliveryEntity>();
        d.derived::<UnitTags>();
        d.derived::<Avatar>();
        d.derived::<ArrivedShort>();
        d.component::<Relations>();
        d.scratch::<NewUnitStates>();
        d.on_insert::<UnitType>(TagBook::give_tags);
        d.cache::<UnitGrid>();
        d.cache::<UnitTracker<GridUnits>>();
        d.observer(UnitTracker::<GridUnits>::aged);
        d.observer(UnitTracker::<GridUnits>::lost_body);
        d.observer(UnitTracker::<GridUnits>::disabled);
        d.observer(UnitTracker::<GridUnits>::enabled);
        d.observer(Lifespan::revived);
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
        d.require::<MoveStep, Leaving>();
        d.fact::<DeclaredTags>(DeclaredTags::build);
        d.fact::<UnitTypes>(UnitTypes::build);
        d.names::<UnitType>(UnitTypes::type_names);
        d.names::<Tag>(UnitTypes::tag_names);
        d.extend::<UnitTypes>(|cx, types| types.tag_bodies(cx));
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
        d.api(UnitsApi::register);
        d.book::<ById<UnitType, MoveStep>>(UnitSteps::steps);
        d.book::<ById<UnitType, TurnStep>>(UnitSteps::turns);
        d.book::<ById<UnitType, AccelStep>>(UnitSteps::accelerations);
        d.book::<ById<UnitType, BodyForm>>(Self::bodies);
        d.book::<ById<UnitType, Variations>>(Variations::book);
        // A unit faces its spawn's angle, which its body turns by.
        d.part::<BodyUnit, Facing>(|at| Some(Facing::of(Facing::angle_of(at))));
        d.part::<BodyUnit, SpawnPoint>(|at| at.position().map(SpawnPoint::new));
        // Every unit holds the engine tags its capabilities turn for what it is now.
        d.part::<BodyUnit, StatusTags>(|_| Some(StatusTags::default()));
        d.part::<BodyUnit, Variation>(Variation::of);
        // A unit of a type that varies takes its variation's body, of the one draw its spawn made;
        // a restored unit of a place past its type's has none, which its variation's check refuses.
        d.part::<BodyUnit, Body>(|at| {
            let form = match Variation::of(at) {
                Some(variation) => at.of_type::<Variations>()?.get(variation.get()),
                None => at.of_type::<BodyForm>(),
            };
            form.map(|form| form.at(Facing::angle_of(at)))
        });
        d.part::<BodyUnit, MoveStep>(|at| at.of_type::<MoveStep>().copied());
        d.part::<BodyUnit, TurnStep>(|at| at.of_type::<TurnStep>().copied());
        d.part::<BodyUnit, AccelStep>(|at| at.of_type::<AccelStep>().copied());
        // A unit that walks, which its move step tells, takes a destination.
        d.part::<BodyUnit, Destination>(|at| {
            at.of_type::<MoveStep>().map(|_| Destination::default())
        });
        d.part::<BodyUnit, Leaving>(|at| at.of_type::<MoveStep>().map(|_| Leaving::default()));
    }

    /// Adds the core to a match: as the tick ends, the units whose lifespan ended despawn, and its
    /// writes to units that never spawned drop.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.scripted(UnitsSystems::End, UnitTicks::end_lifespans);
        cx.scripted(UnitsSystems::Close, UnitTicks::drop_unspawned_states);
    }
}

impl Units {
    /// Every avatar of `view`, living or dead, of `team` or of every team, by stable id.
    pub fn avatars(view: &View, team: Option<Team>, listing: Listing) -> ScriptList {
        view.units_where(|row| {
            listing.lists(row) && Avatar::is_tagged(row) && team.is_none_or(|team| row.team == team)
        })
    }

    /// The body of each unit type that stands with a `collision` or `variations`: its
    /// collision's, or the bound of its variations
    /// (04-capabilities/00-overview.md, A unit type may vary its units).
    fn bodies(cx: &BookCx<'_>) -> Result<ById<UnitType, BodyForm>, BookError> {
        let variations = cx.book::<ById<UnitType, Variations>>()?;
        let mut bodies = ByIdBuilder::default();
        for unit in cx.standing() {
            let form = match unit.data.section::<CollisionData>() {
                Some(collision) => collision.form(cx, unit)?,
                None => match variations.get(unit.unit_type) {
                    Some(variations) => variations.bound(),
                    None => continue,
                },
            };
            bodies.set(unit.unit_type, form);
        }
        Ok(bodies.build())
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
                let holds = |place: &StatePlace| HoldsClass { world, entity }.on(place.audience);
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
}

/// Whether the unit of `entity` holds a class's `UnitState`.
#[derive(Debug)]
struct HoldsClass<'w> {
    world: &'w World,
    entity: Entity,
}

impl AudienceCall for HoldsClass<'_> {
    type Output = bool;

    fn call<S: StateAudience>(self) -> bool {
        self.world.entity(self.entity).contains::<UnitState<S>>()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use bevy_ecs::world::World;
    use campfire_common::DeclaredName;
    use campfire_declarations::internals::BookLoad;
    use campfire_declarations::{ById, PerId};
    use campfire_names::MatchNames;
    use campfire_script::{CallError, CompileError, Ctx, ScriptHost, ScriptId, ScriptValue};
    use campfire_sim::StableId;
    use campfire_values::{Block, PackageIndex, Relation, Scalar, Team, TypeScope, UnitType};

    use crate::script_book::ScriptBook;
    use crate::scripts::Scripts;
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
            let mut types = UnitTypeBook::amend(world, |_, book| book.take_loading());
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
            let books = UnitTypeBook::amend(world, |_, book| book.take_loading()).finish(data);
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
        pub fn give_type<T: PerId<UnitType>>(world: &mut World, unit_type: UnitType, value: T) {
            ById::<UnitType, T>::amend(world, |_, book| book.set(unit_type, value));
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
        pub fn compile_hooked(world: &mut World, source: &str) -> Result<ScriptId, CompileError> {
            let script = Scripts::compile(world, source)?;
            ScriptBook::amend(world, |world, scripts| {
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
