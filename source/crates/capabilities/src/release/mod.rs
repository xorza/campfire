use std::any::TypeId;
use std::collections::BTreeMap;

use bevy_ecs::world::World;
use campfire_abilities::Abilities;
use campfire_actions::Actions;
use campfire_areas::Areas;
use campfire_combat::Combat;
use campfire_common::DeclaredName;
use campfire_declare::{
    BuiltBooks, CapabilitySet, Declarations, InstallFacts, KindVisitor, MatchFacts, ModuleVisitor,
    Role, SectionReaders, UnitTypeData,
};
use campfire_deliveries::Deliveries;
use campfire_interaction::Interaction;
use campfire_items::Items;
use campfire_mode::{Mode, ModeCore};
use campfire_navigation::Navigation;
use campfire_orders::Orders;
use campfire_players::Players;
use campfire_production::Production;
use campfire_progression::Progression;
use campfire_projectiles::Projectiles;
use campfire_script::{ApiBuilder, ScriptApi, ScriptHost, ScriptId};
use campfire_sim::{Capability, SimWorldBuilder, Unpredicted};
use campfire_state_types::StateTypes;
use campfire_stats::Stats;
use campfire_units::Units;
use campfire_values::{EngineTag, NameId};
use campfire_vision::Vision;
use serde::Deserializer;
use serde::de::DeserializeSeed;

use crate::books::Books;
use crate::books::leftover_books::LeftoverBooks;
use crate::books::units_seed::UnitsSeed;
use crate::release::api_binder::ApiBinder;
use crate::release::called_registries::CalledRegistries;
use crate::release::capability_rows::CapabilityRows;
use crate::release::declaring::{Declaring, Scripts};
use crate::release::engine_tag_list::EngineTagList;
use crate::release::error::CapabilityError;
use crate::release::installing::{Installing, Walk};
use crate::release::kinds_only::KindsOnly;
use crate::release::match_core::MatchCore;
use crate::release::match_kind::{MatchKind, ModeStart, PackageScript};
use crate::release::match_spec::MatchSpec;
use crate::release::section_list::SectionList;
use crate::release::state_only::StateOnly;

mod api_binder;
mod book_build;
mod called_registries;
mod capability_rows;
#[cfg(feature = "internals")]
mod client_holds;
mod declaring;
mod engine_tag_list;
pub(crate) mod error;
mod installer;
mod installing;
mod kinds_only;
pub(crate) mod r#match;
mod match_core;
pub(crate) mod match_kind;
pub(crate) mod match_spec;
#[cfg(feature = "internals")]
mod module_sets;
#[cfg(feature = "internals")]
mod resource_audit;
mod section_list;
mod stage_span;
mod state_only;

/// The engine's modules as the release runs them: the core, the mode's core, each capability in
/// the order they install, and the mode's scripted part, which installs once the scripts compile.
#[derive(Debug)]
pub struct Release;

impl Release {
    /// Visits, with `v`, every module a match builds with, in the order they install: what the
    /// match's build puts in itself, the core's and the mode's core, then each capability after
    /// the ones it builds on. Each capability's
    /// orders apply in this order too, the orders of `orders` last. The mode's scripted part
    /// installs above them, once the scripts compile.
    fn modules<V: ModuleVisitor>(v: &mut V) {
        v.module::<MatchCore>();
        v.module::<Units>();
        v.module::<ModeCore>();
        v.module::<Players>();
        v.module::<Actions>();
        v.module::<Deliveries>();
        v.module::<Stats>();
        v.module::<Progression>();
        v.module::<Combat>();
        v.module::<Navigation>();
        v.module::<Vision>();
        v.module::<Projectiles>();
        v.module::<Areas>();
        v.module::<Abilities>();
        v.module::<Production>();
        v.module::<Items>();
        v.module::<Interaction>();
        v.module::<Orders>();
        v.scripted::<Mode>();
    }

    /// The capabilities of the list, each with what it builds on.
    fn rows() -> CapabilityRows {
        let mut rows = CapabilityRows::default();
        Self::modules(&mut rows);
        rows
    }

    /// Every capability the release runs.
    pub fn capabilities() -> CapabilitySet {
        Self::rows()
            .0
            .iter()
            .fold(CapabilitySet::CORE, |set, row| set.with(row.capability))
    }

    /// The set of a mode's `declared` capabilities; an error when one is `mode`, one is declared
    /// twice, one is of no module this release builds, or one lacks a capability it builds on.
    pub fn capability_set(declared: &[Capability]) -> Result<CapabilitySet, CapabilityError> {
        let mut set = CapabilitySet::CORE;
        for &capability in declared {
            if capability == Capability::Mode {
                return Err(CapabilityError::DeclaresMode);
            }
            if set.contains(capability) {
                return Err(CapabilityError::Repeated(capability));
            }
            set = set.with(capability);
        }
        let rows = Self::rows();
        for capability in set.iter() {
            let needs = rows
                .needs(capability)
                .ok_or(CapabilityError::NotBuilt(capability))?;
            if let Some(&needs) = needs.iter().find(|&&needs| !set.contains(needs)) {
                return Err(CapabilityError::Needs { capability, needs });
            }
        }
        Ok(set)
    }

    /// Every engine tag the release's modules declare, in the order of their numbers: the first
    /// tags of every match, whatever its capabilities, so a tag has one number in all.
    pub fn engine_tags() -> Vec<EngineTag> {
        let mut list = EngineTagList::default();
        Self::declare(Self::capabilities(), &mut list);
        for (at, tag) in list.0.iter().enumerate() {
            assert_eq!(
                tag.tag().index(),
                at,
                "{} takes its place among the engine tags",
                tag.name()
            );
        }
        list.0
    }

    /// Every section the release's modules register, of every table, whatever a match's
    /// capabilities, so a table reads alike in every mode and the load refuses a section of a
    /// module its mode does not run.
    pub fn sections() -> SectionReaders {
        let mut list = SectionList::default();
        Self::declare(Self::capabilities(), &mut list);
        list.0
    }

    /// The unit types of a table of them by name, `[units.<name>]`, each read as the sections
    /// the release's modules register read it: the read of a package's units, as serde's
    /// `deserialize_with` takes it.
    pub fn read_units<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<DeclaredName, UnitTypeData>, D::Error> {
        UnitsSeed(&Self::sections()).deserialize(deserializer)
    }

    /// Binds the script API of the release into `host`, as `script_api` records it, so `host`
    /// compiles a script as a match's engine does: with the engine enums' modules.
    pub fn bind_script_api(host: &mut ScriptHost) -> ScriptApi {
        ScriptApi::release(host, Self::bind_api)
    }

    /// Visits, with `d`, the declarations of a match of `set`: each module's of the list that the
    /// match runs, in the order they install, the mode's scripted part's among them.
    pub(crate) fn declare<D: Declarations>(set: CapabilitySet, d: &mut D) {
        Self::modules(&mut Declaring::new(set, Scripts::Visited, d));
    }

    /// The registries of callbacks the modules of a match of `set` and `role` call or declare
    /// callbacks in: the mode's scripted part's only in a match that runs scripts, which
    /// installs it.
    fn called(set: CapabilitySet, role: &Role) -> Vec<TypeId> {
        let mut called = CalledRegistries::default();
        let scripts = if role.scripts() {
            Scripts::Visited
        } else {
            Scripts::Skipped
        };
        Self::modules(&mut Declaring::new(set, scripts, &mut called));
        called.0
    }

    /// Lists, into `types`, the state types of a match of `set`, as its install registers them.
    pub fn state_types<T: StateTypes>(set: CapabilitySet, types: &mut T) {
        Self::declare(set, &mut StateOnly(types));
    }

    /// Visits, with `v`, each kind of entity of a match, whatever its capabilities.
    pub fn kinds<V: KindVisitor>(v: &mut V) {
        Self::declare(Self::capabilities(), &mut KindsOnly(v));
    }

    /// Binds into `builder` the script API of every module the release runs, as the load check
    /// reads it.
    fn bind_api(builder: &mut ApiBuilder<'_>) {
        Self::declare(Self::capabilities(), &mut ApiBinder(builder));
    }

    /// Builds the match of `spec` in the world, a new world, of `books`, as `start` says:
    /// prepares it for the sim, keeps its facts, installs each module of the list the match runs,
    /// in order, its declarations, its books among them, then its systems, so each install reads
    /// its books; binds the script API of the match's modules into the script host the core's
    /// install puts in, and the mode's names into the script view; then starts the match, as
    /// `start` says, and builds its schedule once. A match that ended runs no stage. The world
    /// then holds the match whole: its schedule, its state registry, and its facts.
    pub fn build(world: &mut World, spec: MatchSpec, books: Books, kind: MatchKind<'_>) {
        let role = spec.role;
        if matches!(role, Role::Client) {
            Unpredicted::register(world);
        }
        let mut sim = SimWorldBuilder::new(world, spec.seed, spec.rate);
        #[cfg(feature = "internals")]
        if spec.without_sync_points {
            sim.without_sync_points();
        }
        let set = spec.set;
        let facts = MatchFacts {
            set,
            role,
            out: spec.out,
        };
        let parts = sim.parts();
        parts.world.insert_resource(facts.clone());
        let mode_script = match &kind {
            MatchKind::Mode(start) => Some(start.setup.script),
            MatchKind::Rules | MatchKind::Predicting => None,
        };
        let install = InstallFacts {
            called: Self::called(set, &facts.role),
            facts: &facts,
            mode_script,
        };
        let Books {
            mut built, inputs, ..
        } = books;
        Self::modules(&mut Installing::new(
            parts,
            &mut built,
            &install,
            Walk::Build,
        ));
        Self::declare(Self::capabilities(), &mut LeftoverBooks(&mut built));
        assert!(built.is_empty(), "each built book is declared: {built:?}");
        if facts.role.scripts() {
            let world = sim.parts().world;
            ScriptApi::bind(&mut world.non_send_mut::<ScriptHost>(), |builder| {
                Self::declare(set, &mut ApiBinder(builder));
            });
        }
        match kind {
            MatchKind::Rules => {}
            MatchKind::Predicting => {
                Navigation::load_map(sim.parts().world, inputs.load.walkers);
            }
            MatchKind::Mode(ModeStart { scripts, setup }) => {
                assert!(
                    facts.role.scripts(),
                    "a match that starts its mode runs scripts"
                );
                Self::compile(sim.parts().world, scripts);
                let mut none = BuiltBooks::default();
                let parts = sim.parts();
                Self::modules(&mut Installing::new(
                    parts,
                    &mut none,
                    &install,
                    Walk::Scripted,
                ));
                Mode::load(sim.parts().world, setup, inputs);
            }
        }
        #[cfg(feature = "internals")]
        if spec.unbuilt {
            sim.leave_unbuilt();
            return;
        }
        sim.build();
    }

    /// Compiles `scripts` into the match's script host, each at its place among them, as the
    /// package load parsed them all.
    pub fn compile(world: &mut World, scripts: &[PackageScript<'_>]) {
        for (at, script) in scripts.iter().enumerate() {
            let id = Units::compile(world, script.source)
                .unwrap_or_else(|error| panic!("the load parsed {}: {error:?}", script.path));
            assert_eq!(
                id,
                ScriptId::nth(at),
                "the host numbers {} at its place",
                script.path
            );
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::any::TypeId;
    use std::collections::{BTreeMap, BTreeSet};
    use std::fmt::Debug;
    use std::num::NonZeroU64;

    use bevy_ecs::archetype::Archetype;
    use bevy_ecs::component::{Component, Mutable};
    use bevy_ecs::schedule::InternedSystemSet;
    use bevy_ecs::world::World;
    use campfire_declare::{
        BuiltBooks, CapabilitySet, InstallFacts, MatchFacts, Sections, SourceKind, SourcePackage,
    };
    use campfire_geometry::Bounds;
    use campfire_math::Num;
    use campfire_mode::{MapData, Mode, ModeData, ModeInputs, ModeSetup};
    use campfire_script::{ScriptApi, ScriptHost};
    use campfire_sim::TickRate;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{
        Capability, EntityIndex, SimResource, SimWorld, SimWorldBuilder, SnapshotError, StableId,
        StateCopy, StateRegistry,
    };
    use campfire_state_types::{Replication, StateTypes};
    use campfire_units::{EntityKinds, ScriptBook, UnitTypes, Units, UnitsWorld};
    use campfire_values::{MessageId, Speed};

    use crate::books::Books;
    use crate::books::book_input::BookInput;
    use crate::fixtures::units::UnitsFixtures;
    use crate::release::Release;
    use crate::release::client_holds::ClientHolds;
    use crate::release::installing::{Installing, Walk};
    use crate::release::r#match::Match;
    use crate::release::module_sets::ModuleSets;
    use crate::release::resource_audit::DeclaredResources;

    /// The component types the lists declare, as state or as derived, by their ids.
    #[derive(Debug, Default)]
    struct ListedTypes(BTreeSet<TypeId>);

    impl ListedTypes {
        fn push<C: Component>(&mut self) {
            self.0.insert(TypeId::of::<C>());
        }
    }

    impl StateTypes for ListedTypes {
        fn component<C: Replication>(&mut self) {
            self.push::<C>();
        }

        fn derived<C: Component>(&mut self) {
            self.push::<C>();
        }

        fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
            self.push::<C>();
        }

        fn resource<R: SimResource + Default>(&mut self) {}

        fn sim_component<C: Replication>(&mut self) {
            self.push::<C>();
        }
    }

    impl Release {
        /// The books of a match at `rate` of an empty mode package alone, as a test's or a bench's
        /// starts from: each built from a mode of no data, on a planar map of the world's bounds,
        /// with no team, stat or script; but the unit types, the engine's tags alone, open as
        /// `UnitTypes::open_in` puts them.
        pub fn empty_books(rate: TickRate) -> Books {
            let data = ModeData::named(MessageId::new("mode").unwrap());
            let (units, content) = (BTreeMap::new(), Sections::default());
            let mode = SourcePackage {
                name: "mode",
                kind: SourceKind::Mode,
                units: &units,
                content: &content,
                scripts: Vec::new(),
            };
            let input = BookInput {
                data: &data,
                map: &MapData::planar(Bounds::WORLD),
                heights: None,
                teams: &[],
                max_move_speed: Speed::new(Num::MAX).unwrap(),
                set: Self::capabilities(),
                engine_tags: &Self::engine_tags(),
                tag_names: &[],
                packages: vec![mode],
                scripts: &ScriptBook::default(),
                rate,
                stat_order: &[],
            };
            let mut books = Self::books(&input).expect("a mode of no data builds");
            UnitTypes::new(Self::engine_tags()).open_in(&mut books.built);
            books
        }

        /// Installs the mode's scripted part into `world`, a match a test built with no mode and
        /// whose books the test loaded piece by piece since, and loads the mode of `setup` with
        /// what `inputs` takes from the books: the tick's schedule opens again, and builds again.
        pub fn install_mode(world: &mut World, setup: ModeSetup<'_>, inputs: ModeInputs) {
            let facts = world.resource::<MatchFacts>().clone();
            assert!(
                facts.role.scripts(),
                "the mode installs into a match that runs scripts"
            );
            let mut sim = SimWorldBuilder::reopen(world);
            let install = InstallFacts {
                called: Self::called(facts.set, &facts.role),
                facts: &facts,
                mode_script: Some(setup.script),
            };
            let mut none = BuiltBooks::default();
            let parts = sim.parts();
            Self::modules(&mut Installing::new(
                parts,
                &mut none,
                &install,
                Walk::Scripted,
            ));
            Mode::load(sim.parts().world, setup, inputs);
            sim.build();
        }

        /// The script API of the release, recorded as a match's engine binds it, with the names
        /// the engine has before: every capability's, whether a mode declares it or not.
        pub fn script_api() -> ScriptApi {
            Self::bind_script_api(&mut ScriptHost::new(NonZeroU64::MIN))
        }

        /// Every system set the modules of `set` declare, in the order they declare them: the
        /// sets each game system of a match of `set` sits in.
        pub fn module_sets(set: CapabilitySet) -> Vec<InternedSystemSet> {
            let mut sets = ModuleSets::default();
            Self::declare(set, &mut sets);
            sets.0
        }

        /// The names of the resources of `world`, a match of `set`, that no declaration names,
        /// `host`'s own aside, then, while the match runs, of each scratch resource not clear: a
        /// resource none declares may carry from tick to tick what no hash, snapshot or restore
        /// holds.
        pub fn unlisted_resources(
            set: CapabilitySet,
            world: &World,
            host: &[TypeId],
        ) -> Vec<String> {
            DeclaredResources::audit(set, world, host)
        }

        /// The stable ids of the entities of `world` that are of no kind: each entity the state
        /// registry indexes holds the marker of one kind.
        pub fn entities_of_no_kind(world: &World) -> Vec<StableId> {
            let kinds = world.resource::<EntityKinds>();
            world
                .resource::<EntityIndex>()
                .iter()
                .filter(|&(_, entity)| kinds.of(world.entity(entity)).is_none())
                .map(|(id, _)| id)
                .collect()
        }

        /// The names of the component types a unit of `world`, an entity with a stable id, holds
        /// or held, that no list of the capabilities `set` declares as state or as derived, sorted:
        /// each a part of a unit that no hash, snapshot or client holds. The stable id is the
        /// entity list's, which the state registry holds.
        pub fn unlisted_components(set: CapabilitySet, world: &World) -> Vec<String> {
            let mut listed = ListedTypes::default();
            Self::state_types(set, &mut listed);
            listed.push::<StableId>();
            let components = world.components();
            let Some(unit) = components.component_id::<StableId>() else {
                return Vec::new();
            };
            let mut names: Vec<String> = world
                .archetypes()
                .iter()
                .filter(|archetype| archetype.contains(unit))
                .flat_map(Archetype::components)
                .filter_map(|&id| components.get_info(id))
                .filter(|info| info.type_id().is_none_or(|id| !listed.0.contains(&id)))
                .map(|info| info.name().to_string())
                .collect();
            names.sort_unstable();
            names.dedup();
            names
        }
    }

    impl Release {
        /// The rule (14-replication-audiences.md, Prediction reads only what its client holds) for
        /// the reads no access names, an exclusive system's, as GGPO's sync test checks rollback:
        /// `scene` places a scene into a predicting client's match of `declared` twice, each unit
        /// it does not predict marked `Unpredicted`; in the second, those units hold only the state
        /// types their audiences send the client. Both run `ticks` ticks of alike inputs, so the
        /// units the client predicts end alike, and each unit it does not predict keeps what it
        /// received, unwritten: a read of state the client lacks shows as a difference.
        pub fn sync_held(declared: &[Capability], ticks: u64, scene: impl Fn(&mut Match)) {
            let set = Self::capability_set(declared).expect("a test declares a valid set");
            let mut worlds = [(); 2].map(|()| {
                let mut game = Match::predicting(declared);
                scene(&mut game);
                let holds = ClientHolds::of(&mut game, set);
                (game, holds)
            });
            let [_, (held, held_holds)] = &mut worlds;
            held_holds.strip(held);
            for (game, holds) in &mut worlds {
                let parts = holds.parts(game);
                let since = game.read_change_tick();
                for _ in 0..ticks {
                    game.step();
                }
                assert_eq!(holds.parts(game), parts, "held units keep their parts");
                let writes = holds.writes(game, since);
                assert_eq!(writes, [], "writes of units held as sent");
            }
            let by_type = worlds.map(|(mut game, _)| {
                for entity in ClientHolds::unpredicted(&mut game) {
                    game.despawn(entity);
                }
                let mut by_type = Vec::new();
                game.state_hash_by_type(&mut by_type);
                by_type
            });
            let [full, held] = &by_type;
            debug_assert_eq!(full.len(), held.len(), "one match's state types");
            let differ: Vec<&str> = full
                .iter()
                .zip(held)
                .filter(|(full, held)| full.hash != held.hash)
                .map(|(full, _)| full.name)
                .collect();
            assert_eq!(
                differ,
                Vec::<&str>::new(),
                "the units the client predicts end alike with what it holds"
            );
        }

        /// Restores its state into `fresh`, a match that loaded the same books, as a restore does; the
        /// error that refuses it, if one does. `fresh`'s unit types seal as a spawn seals them,
        /// and it loads the modifiers of the test tags when `world` did.
        pub fn refusal_into(world: &World, fresh: &mut World) -> Option<SnapshotError> {
            Units::seal_once(fresh);
            if Units::loads_test_modifiers(world) {
                Units::load_test_modifiers(fresh);
            }
            Self::clear(fresh);
            let mut snapshot = Vec::new();
            world.snapshot(&mut snapshot);
            fresh.restore(&snapshot).err()
        }

        /// Despawns the match's entity it started with, as a restore takes a world of no entity.
        pub fn clear(world: &mut World) {
            if let Some(entity) = world.match_entity() {
                world.despawn(entity);
            }
        }

        /// Restores its state into `fresh`, a match that loaded the same books, as a restore does, and
        /// asserts that the two hash the same.
        pub fn restore_into(world: &World, fresh: &mut World) {
            assert_eq!(
                Self::refusal_into(world, fresh),
                None,
                "a match's own snapshot restores"
            );
            assert_eq!(
                fresh.state_hash(),
                world.state_hash(),
                "the restored match hashes as the match"
            );
        }

        /// Brings `copy`, a copy of the match's state, up to the match, and checks that every state
        /// type of it hashes as the match's own.
        pub fn check_copy(world: &mut World, copy: &mut StateCopy) {
            copy.follow(world);
            let (mut copied, mut own) = (Vec::new(), Vec::new());
            let registry = world.resource::<StateRegistry>();
            registry.hash_by_type(copy.world(), &mut copied);
            registry.hash_by_type(world, &mut own);
            let differ: Vec<&str> = copied
                .iter()
                .zip(&own)
                .filter(|(copied, own)| copied.hash != own.hash)
                .map(|(copied, _)| copied.name)
                .collect();
            assert!(
                differ.is_empty(),
                "the copy differs before tick {}: {differ:?}",
                world.now().get()
            );
        }
    }
}
#[cfg(test)]
mod tests;
