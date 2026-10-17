//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use std::sync::Arc;

use campfire_common::DeclaredName;
use campfire_declare::ByType;
use campfire_mode::ModeInputs;
use campfire_navigation::{MapProblem, WalkCheck, Walker};
use campfire_units::{BodyForm, MapBook, TypesError, UnitTypeBook, UnitTypes};
use campfire_values::{DamageKind, EngineTag, Names, PackageIndex, ResourceId, UnitType};

use crate::books::book_builder::BookBuilder;
use crate::books::book_input::{BookInput, BookPackage};
use crate::books::built_books::BuiltBooks;
use crate::books::error::BookError;

pub(crate) mod book_builder;
pub(crate) mod book_input;
pub(crate) mod book_parts;
pub(crate) mod book_place;
pub(crate) mod built_books;
pub(crate) mod error;
#[cfg(feature = "internals")]
pub(crate) mod every_book;
pub(crate) mod leftover_books;
pub(crate) mod package_content;
pub(crate) mod type_place;
pub(crate) mod unit_start;
pub(crate) mod units_seed;

/// The books of a match, built once from its packages at its tick rate, each until the build
/// puts it where its owner declares it, before its owner installs; the names of the mode's damage
/// kinds and players' resources, which the script view holds; and what the mode's load takes.
#[derive(Debug)]
pub struct Books {
    pub(crate) built: BuiltBooks,
    pub(crate) damage_kinds: Arc<Names<DamageKind>>,
    pub(crate) resources: Arc<Names<ResourceId>>,
    /// The tags of the markers the mode's units spawn at, which the map's walk check reads.
    pub(crate) spawn_tags: Box<[DeclaredName]>,
    pub(crate) inputs: ModeInputs,
}

impl Books {
    /// The books of `input`, which the package load checked; an error for what the check does
    /// not see and the books cannot hold.
    pub fn build(input: &BookInput<'_>) -> Result<Self, BookError> {
        BookBuilder::new(input)?.build()
    }

    /// The unit types of `packages`, the mode's then each dependency's, and the tags of the
    /// match: `engine_tags`, the release's, then `tag_names`, then those the types name, in the
    /// order their ids follow; an error for a type its scope names already, or past the most
    /// types or tags a match holds. The build's first step, and the tables a check of the
    /// packages resolves their names by.
    pub fn unit_types(
        engine_tags: &[EngineTag],
        tag_names: &[DeclaredName],
        packages: &[BookPackage<'_>],
    ) -> Result<UnitTypes, TypesError> {
        let mut types = UnitTypes::new(engine_tags.to_vec());
        for name in tag_names {
            types.declare(name.as_str())?;
        }
        for (index, package) in (0..).map(PackageIndex::new).zip(packages) {
            BookBuilder::declare_types(&mut types, index, package)?;
        }
        Ok(types)
    }

    /// Checks that the mode's map can be walked by the kinds of its unit types that walk, as
    /// `WalkCheck::run` says, among its placed units of the types that do not walk.
    pub fn check_walkable(&self) -> Result<(), MapProblem> {
        let standing = &self.inputs.units.unit_types;
        let walkers = self
            .built
            .get::<ByType<Walker>>()
            .expect("the walkers are built");
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
        let navigation = self.built.get().expect("the paths and walls are built");
        let load = &self.inputs.load;
        let check = WalkCheck {
            ground,
            navigation,
            cells: load.map.pathing,
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
