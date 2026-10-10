//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use std::mem;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

use crate::actions::actions_column::ActionsColumn;
use crate::books::book_builder::BookBuilder;
use crate::books::book_input::BookInput;
use crate::books::book_parts::BookParts;
use crate::books::error::BookError;
use crate::books::mode_inputs::ModeInputs;
use crate::mode::mode_books::ModeBooks;
use crate::mode::mode_map::ModeMap;
use crate::navigation::error::MapProblem;
use crate::production::production_column::ProductionColumn;
use crate::production::supply_costs::SupplyCosts;
use crate::progression::progression_column::ProgressionColumn;
use crate::scripts::ctx::Ctx;
use crate::stats::param_book::ParamBook;
use crate::stats::stats_call::StatsCall;
use crate::stats::stats_column::StatsColumn;
use crate::units::predicting::Predicting;
use crate::units::unit_type::UnitType;
use crate::units::units_column::UnitsColumn;
use crate::units::view::View;
use crate::values::declared_name::DeclaredName;

pub(crate) mod book_builder;
pub(crate) mod book_input;
pub(crate) mod book_parts;
pub(crate) mod error;
pub(crate) mod mode_inputs;
pub(crate) mod package_content;
pub(crate) mod type_place;
pub(crate) mod unit_type_file;

/// The books of a match, built once from its packages at its tick rate: what the builder loads
/// package by package, and the books of the mode's own rules, which read the unit types it
/// loaded.
#[derive(Debug)]
pub struct Books {
    parts: BookParts,
    mode: ModeBooks,
}

impl Books {
    /// Puts `book` in place of the resource of its type in `world`, which the capability that
    /// reads it installed; none when the match does not have that capability, whose book is then
    /// empty.
    fn replace<T: Resource>(world: &mut World, book: T) {
        if world.contains_resource::<T>() {
            world.insert_resource(book);
        }
    }

    /// The books of `input`, which the package load checked; an error for what the check does
    /// not see and the books cannot hold.
    pub fn build(input: &BookInput<'_>) -> Result<Books, BookError> {
        BookBuilder::new(input).build()
    }

    /// Checks that the mode's map can be walked by the kinds of its unit types that walk, as
    /// `ModeMap::check_walkable` says, among its placed units of the types that do not walk.
    pub fn check_walkable(&self) -> Result<(), MapProblem> {
        let kits = &self.parts.units.unit_types;
        let body_of = |unit_type| {
            let walks = self.parts.walkers.get(unit_type).is_some();
            let placed = kits.iter().find(|setup| setup.unit_type == unit_type);
            placed
                .expect("a placed unit's type stands")
                .kit
                .body
                .filter(|_| !walks)
        };
        let name_of = |unit_type: UnitType| {
            let name = self.parts.types.names().nth(unit_type.index());
            DeclaredName::new(name.expect("a type of the books"))
                .expect("a type's name is declared")
        };
        let walkers = &self.mode.walkers;
        self.mode.map.check_walkable(walkers, body_of, name_of)
    }

    /// Puts the books in `world`, a match whose capabilities are installed and whose scripts are
    /// compiled, each in the resource of the capability that reads it, and the view's and, with
    /// scripts, the frame's copies; what the mode's book takes stays.
    pub fn install(self, world: &mut World) -> ModeInputs {
        let Books { parts, mode } = self;
        let view = world.non_send::<View>().clone();
        let states = parts.types.state_book();
        UnitsColumn::share(&view, states.clone());
        world.insert_resource(states);
        world.insert_resource(parts.types.origins());
        view.set_types(parts.types);
        StatsColumn::share_modifiers(&view, parts.modifiers.clone());
        let params = ParamBook::new(parts.params);
        if let Some(ctx) = world.get_non_send::<Ctx>() {
            StatsCall::share_params(&mut ctx.frame(), params.clone());
        }
        world.insert_resource(params);
        if let Some(tracks) = parts.tracks {
            ProgressionColumn::share(&view, tracks.clone());
            world.insert_resource(tracks);
        }
        ActionsColumn::share(&view, parts.actions.clone());
        let costs = SupplyCosts::new(parts.supplies, &parts.actions);
        Books::replace(world, parts.modifiers);
        Books::replace(world, parts.actions);
        Books::replace(world, parts.effects);
        Books::replace(world, parts.ais);
        Books::replace(world, parts.projectiles);
        Books::replace(world, parts.areas);
        Books::replace(world, parts.sights);
        Books::replace(world, parts.producers);
        Books::replace(world, parts.requirements);
        Books::replace(world, parts.builds);
        Books::replace(world, parts.nodes);
        Books::replace(world, parts.walkers);
        ProductionColumn::share(&view, parts.supply_rules, costs.clone());
        Books::replace(world, costs);
        if let Some(rules) = parts.supply_rules {
            world.insert_resource(rules);
        }
        Books::replace(world, parts.items);
        ModeInputs {
            units: parts.units,
            books: mode,
        }
    }

    /// Puts the books in `world`, a client's, whose capabilities are installed with no scripts,
    /// and the part of the mode no script runs, as a match's mode install puts them: the books
    /// of its own rules, and its map's ground, its pathing grid for the kinds of its walkers. The
    /// client then predicts its units by the rules the server runs: it starts their actions, and
    /// runs none of their effects.
    pub fn install_prediction(self, world: &mut World) {
        let ModeInputs { mut books, .. } = self.install(world);
        let walkers = mem::take(&mut books.walkers);
        let ModeMap { ground, .. } = books.install(world);
        ground.install(world, walkers);
        world.insert_resource(Predicting);
    }
}
