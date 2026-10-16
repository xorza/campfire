//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_declare::ByType;
use campfire_mode::ModeInputs;
use campfire_navigation::{MapProblem, WalkCheck, Walker};
use campfire_script::View;
use campfire_units::{BodyForm, MapBook, TypesError, UnitTypeBook, UnitTypes};
use campfire_values::{DamageKind, DeclaredName, Names, PackageIndex, ResourceId, UnitType};

use crate::books::book_builder::BookBuilder;
use crate::books::book_fill::BookFill;
use crate::books::book_input::{BookInput, BookPackage};
use crate::books::built_books::BuiltBooks;
use crate::books::error::BookError;
use crate::release::Release;

pub(crate) mod book_builder;
pub(crate) mod book_fill;
pub(crate) mod book_input;
pub(crate) mod book_parts;
pub(crate) mod book_place;
pub(crate) mod built_books;
pub(crate) mod error;
pub(crate) mod package_content;
pub(crate) mod type_place;
pub(crate) mod unit_start;
pub(crate) mod unit_type_data;

/// The books of a match, built once from its packages at its tick rate, each until the install
/// puts it where its owner declares it; the names of the mode's damage kinds and players'
/// resources, which the script view holds; and what the mode's install takes.
#[derive(Debug)]
pub struct Books {
    built: BuiltBooks,
    damage_kinds: Arc<Names<DamageKind>>,
    resources: Arc<Names<ResourceId>>,
    /// The tags of the markers the mode's units spawn at, which the map's walk check reads.
    spawn_tags: Box<[DeclaredName]>,
    inputs: ModeInputs,
}

impl Books {
    /// The books of `input`, which the package load checked; an error for what the check does
    /// not see and the books cannot hold.
    pub fn build(input: &BookInput<'_>) -> Result<Self, BookError> {
        BookBuilder::new(input)?.build()
    }

    /// The unit types of `packages`, the mode's then each dependency's, and the tags of the
    /// match: the engine's, then `tag_names`, then those the types name, in the order their ids
    /// follow; an error for a type its scope names already, or past the most types or tags a
    /// match holds. The build's first step, and the tables a check of the packages resolves
    /// their names by.
    pub fn unit_types(
        tag_names: &[DeclaredName],
        packages: &[BookPackage<'_>],
    ) -> Result<UnitTypes, TypesError> {
        let mut types = UnitTypes::new(Release::engine_tags());
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

    /// Puts the books in `world`, a match whose capabilities are installed and whose scripts are
    /// compiled: the mode's names in the script view, then each book its owner declares, where
    /// its install put an empty one. A book declared and not built, or built and not declared, is
    /// a logic error.
    pub fn install(self, world: &mut World) -> ModeInputs {
        let Self {
            mut built,
            damage_kinds,
            resources,
            inputs,
            ..
        } = self;
        {
            let view = world.non_send::<View>();
            let mut names = view.names_mut();
            names.set_table(damage_kinds);
            names.set_table(resources);
        }
        Self::fill(&mut built, world);
        inputs
    }

    /// Takes each book every module declares out of `built`, which must hold it, and puts it in
    /// `world` where the match holds it; `built` must hold no other book.
    fn fill(built: &mut BuiltBooks, world: &mut World) {
        let mut fill = BookFill { built, world };
        Release::declare(Release::capabilities(), &mut fill);
        assert!(built.is_empty(), "each built book is declared: {built:?}");
    }

    /// Puts the books in `world`, a client's, whose capabilities are installed with no scripts,
    /// and the map's pathing grid for the kinds of its walkers, as a match's mode install puts
    /// it. The client then predicts its units by the rules the server runs: it starts their
    /// actions, and runs none of their effects.
    pub fn install_prediction(self, world: &mut World) {
        let ModeInputs { load, .. } = self.install(world);
        load.map.install_pathing(world, load.walkers);
    }
}

#[cfg(test)]
mod tests;
