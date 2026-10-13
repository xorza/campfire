//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use std::any::type_name;
use std::sync::Arc;

use bevy_ecs::world::World;

use crate::books::book_builder::BookBuilder;
use crate::books::book_fill::BookFill;
use crate::books::book_input::BookInput;
use crate::books::built_books::BuiltBooks;
use crate::books::error::BookError;
use crate::declare::book::Book;
use crate::declare::capability_set::CapabilitySet;
use crate::mode::mode_inputs::ModeInputs;
use crate::navigation::error::MapProblem;
use crate::navigation::walker::Walker;
use crate::release::Release;
use crate::scripts::ctx::Ctx;
use crate::units::body_form::BodyForm;
use crate::units::by_type::ByType;
use crate::units::map_book::MapBook;
use crate::units::resource_id::ResourceId;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::units::view::View;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::names::Names;

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
/// puts it where its owner declares it; the unit types and tags, which the script view holds;
/// the names of the mode's damage kinds and players' resources; and what the mode's install takes.
#[derive(Debug)]
pub struct Books {
    built: BuiltBooks,
    types: UnitTypes,
    damage_kinds: Arc<Names<DamageKind>>,
    resources: Arc<Names<ResourceId>>,
    inputs: ModeInputs,
}

impl Books {
    /// Puts `book` in place of the empty one its owner's declaration put in `world`, and gives
    /// the script view and, in a match that runs scripts, the call frame what they copy of it:
    /// the step of the install for each book, which a test that loads a book alone takes too.
    pub(crate) fn load<B: Book>(world: &mut World, book: B) {
        assert!(
            world.contains_resource::<B>(),
            "{} is declared by its owner",
            type_name::<B>()
        );
        world.insert_resource(book);
        Books::share::<B>(world);
    }

    /// Gives the script view and, in a match that runs scripts, the call frame what they copy of
    /// `world`'s book `B`, as it stands.
    pub(crate) fn share<B: Book>(world: &World) {
        let view = world.non_send::<View>();
        let ctx = world.get_non_send::<Ctx>();
        let mut frame = ctx.map(Ctx::frame);
        world.resource::<B>().share(view, frame.as_deref_mut());
    }

    /// The books of `input`, which the package load checked; an error for what the check does
    /// not see and the books cannot hold.
    pub fn build(input: &BookInput<'_>) -> Result<Books, BookError> {
        BookBuilder::new(input)?.build()
    }

    /// Checks that the mode's map can be walked by the kinds of its unit types that walk, as
    /// `ModeMap::check_walkable` says, among its placed units of the types that do not walk.
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
            let stands = standing.iter().any(|setup| setup.unit_type == unit_type);
            assert!(stands, "a placed unit's type stands");
            let walks = walkers.get(unit_type).is_some();
            bodies.get(unit_type).copied().filter(|_| !walks)
        };
        let name_of = |unit_type: UnitType| {
            let name = self.types.types().name(unit_type);
            DeclaredName::new(name.expect("a type of the books"))
                .expect("a type's name is declared")
        };
        let ground = self.built.get::<MapBook>().expect("the ground is built");
        let navigation = self.built.get().expect("the paths and walls are built");
        let load = &self.inputs.load;
        load.map
            .check_walkable(ground, navigation, &load.walkers, body_of, name_of)
    }

    /// Puts the books in `world`, a match of the capabilities `set` that runs scripts when
    /// `scripts` says, whose capabilities are installed and whose scripts are compiled: the unit
    /// types and the mode's names in the script view, then each book its owner declares, when
    /// the match holds it. A book declared and not built, or built and not declared, is a logic
    /// error.
    pub fn install(self, world: &mut World, set: CapabilitySet, scripts: bool) -> ModeInputs {
        let Books {
            mut built,
            types,
            damage_kinds,
            resources,
            inputs,
        } = self;
        {
            let view = world.non_send::<View>();
            let mut names = view.names_mut();
            names.set_types(types);
            names.set_mode_names(damage_kinds, resources);
        }
        Books::fill(&mut built, world, set, scripts);
        inputs
    }

    /// Takes each book every module declares out of `built`, which must hold it, and puts it in
    /// `world`, a match of `set` that runs scripts when `scripts` says, when the match holds it;
    /// `built` must hold no other book.
    fn fill(built: &mut BuiltBooks, world: &mut World, set: CapabilitySet, scripts: bool) {
        let mut fill = BookFill {
            built,
            world,
            set,
            scripts,
        };
        Release::declare(Release::capabilities(), &mut fill);
        assert!(built.is_empty(), "each built book is declared: {built:?}");
    }

    /// Puts the books in `world`, a client's, whose capabilities are installed with no scripts,
    /// and the map's pathing grid for the kinds of its walkers, as a match's mode install puts
    /// it. The client then predicts its units by the rules the server runs: it starts their
    /// actions, and runs none of their effects.
    pub fn install_prediction(self, world: &mut World, set: CapabilitySet) {
        let ModeInputs { load, .. } = self.install(world, set, false);
        load.map.install_pathing(world, load.walkers);
    }
}

#[cfg(test)]
mod tests;
