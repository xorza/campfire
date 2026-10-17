//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use std::num::NonZeroU32;

use campfire_common::DeclaredName;
use campfire_declare::{
    BookCx, BookError, BookSource, BuiltBooks, ByType, CapabilitySet, Given, Sections,
    SourcePackage,
};
use campfire_mode::ModeInputs;
use campfire_navigation::{MapProblem, NavigationBook, WalkCheck, Walker};
use campfire_sim::TickRate;
use campfire_units::{BodyForm, MapBook, UnitTypeBook, UnitTypes};
use campfire_values::{EngineTag, UnitType};

pub(crate) mod book_cells;
pub(crate) mod book_input;
pub(crate) mod leftover_books;
pub(crate) mod package_content;
pub(crate) mod units_seed;

/// The books of a match, built once from its packages at its tick rate, each until the build
/// puts it where its owner declares it, before its owner installs; and what the mode's load
/// takes.
#[derive(Debug)]
pub struct Books {
    pub(crate) built: BuiltBooks,
    /// The tags of the markers the mode's units spawn at, which the map's walk check reads.
    pub(crate) spawn_tags: Box<[DeclaredName]>,
    pub(crate) inputs: ModeInputs,
}

impl Books {
    /// The unit types of `packages`, the mode's then each dependency's, and the tags of the
    /// match: `engine_tags`, the release's, then `tag_names`, then those the types name, in the
    /// order their ids follow; an error for a type its scope names already, or past the most
    /// types or tags a match holds. The tables a check of the packages resolves their names by.
    pub fn unit_types(
        engine_tags: &[EngineTag],
        tag_names: &[DeclaredName],
        packages: &[SourcePackage<'_>],
    ) -> Result<UnitTypes, BookError> {
        let none = Sections::default();
        let source = BookSource {
            rate: TickRate::new(NonZeroU32::MIN),
            set: CapabilitySet::CORE,
            engine_tags,
            tag_names,
            mode: &none,
            map: &none,
            packages: packages.to_vec(),
            given: Given::default(),
        };
        UnitTypes::build(&BookCx::new(source))
    }

    /// Checks that the mode's map can be walked by the kinds of its unit types that walk, as
    /// `WalkCheck::run` says, among its placed units of the types that do not walk.
    pub fn check_walkable(&self) -> Result<(), MapProblem> {
        let standing = &self.inputs.units.unit_types;
        let none = ByType::default();
        let walkers = self.built.get::<ByType<Walker>>().unwrap_or(&none);
        let bodies = self
            .built
            .get::<ByType<BodyForm>>()
            .expect("the bodies are built");
        let body_of = |unit_type| {
            let stands = standing.contains(&unit_type);
            assert!(stands, "a placed unit's type stands");
            let walks = walkers.get(unit_type).is_some();
            bodies.get(unit_type).copied().filter(|_| !walks)
        };
        let types = self
            .built
            .get::<UnitTypeBook>()
            .expect("the unit types are built");
        let name_of = |unit_type: UnitType| {
            let name = types.types().name(unit_type);
            DeclaredName::new(name.expect("a type of the books"))
                .expect("a type's name is declared")
        };
        let ground = self.built.get::<MapBook>().expect("the ground is built");
        let no_navigation = NavigationBook::default();
        let navigation = self.built.get().unwrap_or(&no_navigation);
        let load = &self.inputs.load;
        let check = WalkCheck {
            ground,
            navigation,
            walkers: &load.walkers,
            placed: &load.map.placed_bodies(body_of),
            spawns: &load.map.spawn_points(&self.spawn_tags),
        };
        check.run(name_of)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_mode::ModeUnits;

    use crate::books::Books;

    impl Books {
        /// The units the mode spawns, which a test that starts no mode spawns itself.
        pub const fn mode_units(&self) -> &ModeUnits {
            &self.inputs.units
        }
    }
}
