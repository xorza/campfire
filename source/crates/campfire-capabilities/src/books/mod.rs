//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use std::any::type_name;
use std::mem;

use bevy_ecs::world::World;

use crate::actions::actions_column::ActionsColumn;
use crate::books::book_builder::BookBuilder;
use crate::books::book_input::BookInput;
use crate::books::book_parts::BookParts;
use crate::books::error::BookError;
use crate::books::mode_inputs::ModeInputs;
use crate::declare::book::Book;
use crate::declare::capability_set::CapabilitySet;
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
    /// Puts `book` in place of its empty default in `world`, which its owner's declaration put
    /// there, when the match declares its owner, `set` says, and runs the scripts it needs. Else
    /// the book stays out: the load leaves a capability the mode does not declare no data, and a
    /// client runs no scripts.
    fn put<B: Book>(world: &mut World, set: CapabilitySet, scripts: bool, book: B) {
        if !B::held(set, scripts) {
            return;
        }
        assert!(
            world.contains_resource::<B>(),
            "{} is declared by its owner",
            type_name::<B>()
        );
        world.insert_resource(book);
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

    /// Puts the books in `world`, a match of the capabilities `set` that runs scripts when
    /// `scripts` says, whose capabilities are installed and whose scripts are compiled, each in
    /// the resource of the module that declares it, and the view's and, with scripts, the frame's
    /// copies; what the mode's book takes stays.
    pub fn install(self, world: &mut World, set: CapabilitySet, scripts: bool) -> ModeInputs {
        let Books { parts, mode } = self;
        let view = world.non_send::<View>().clone();
        let states = parts.types.state_book();
        UnitsColumn::share(&view, states.clone());
        Books::put(world, set, scripts, states);
        Books::put(world, set, scripts, parts.types.origins());
        view.set_types(parts.types);
        StatsColumn::share_modifiers(&view, parts.modifiers.clone());
        let params = ParamBook::new(parts.params);
        if let Some(ctx) = world.get_non_send::<Ctx>() {
            StatsCall::share_params(&mut ctx.frame(), params.clone());
        }
        Books::put(world, set, scripts, params);
        if let Some(tracks) = parts.tracks {
            ProgressionColumn::share(&view, tracks.clone());
            Books::put(world, set, scripts, tracks);
        }
        ActionsColumn::share(&view, parts.actions.clone());
        let costs = SupplyCosts::new(parts.supplies, &parts.actions);
        Books::put(world, set, scripts, parts.modifiers);
        Books::put(world, set, scripts, parts.actions);
        Books::put(world, set, scripts, parts.effects);
        Books::put(world, set, scripts, parts.ais);
        Books::put(world, set, scripts, parts.projectiles);
        Books::put(world, set, scripts, parts.areas);
        Books::put(world, set, scripts, parts.sights);
        Books::put(world, set, scripts, parts.producers);
        Books::put(world, set, scripts, parts.requirements);
        Books::put(world, set, scripts, parts.builds);
        Books::put(world, set, scripts, parts.nodes);
        Books::put(world, set, scripts, parts.walkers);
        ProductionColumn::share(&view, parts.supply_rules, costs.clone());
        Books::put(world, set, scripts, costs);
        if let Some(rules) = parts.supply_rules {
            world.insert_resource(rules);
        }
        Books::put(world, set, scripts, parts.items);
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
    pub fn install_prediction(self, world: &mut World, set: CapabilitySet) {
        let ModeInputs { mut books, .. } = self.install(world, set, false);
        let walkers = mem::take(&mut books.walkers);
        let ModeMap { ground, .. } = books.install(world);
        ground.install(world, walkers);
        world.insert_resource(Predicting);
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{self, AssertUnwindSafe};

    use campfire_sim::Capability;

    use super::*;
    use crate::orders::ai::Ai;
    use crate::release::Release;
    use crate::stats::modifier_book::ModifierBook;
    use crate::units::by_type::ByType;

    #[test]
    fn a_book_goes_in_only_for_its_owner_and_the_scripts_it_needs() {
        let stats = Release::capability_set(&[Capability::Stats]).unwrap();
        let none = Release::capability_set(&[]).unwrap();
        let orders = Release::capability_set(&[
            Capability::Stats,
            Capability::Combat,
            Capability::Navigation,
            Capability::Orders,
        ])
        .unwrap();
        // A capability's book is held with its owner; an AI's with orders and scripts both; a
        // core book in every match.
        assert!(ModifierBook::held(stats, false) && !ModifierBook::held(none, true));
        assert!(ByType::<Ai>::held(orders, true) && !ByType::<Ai>::held(orders, false));
        assert!(!ByType::<Ai>::held(stats, true));
        assert!(ParamBook::held(none, false));
        // A book of an owner the match lacks stays out; one its owner holds goes in, in place of
        // the empty one its declaration put; one its installed owner did not declare is a logic
        // error.
        let mut world = World::new();
        Books::put(&mut world, none, true, ModifierBook::default());
        assert!(!world.contains_resource::<ModifierBook>());
        world.insert_resource(ParamBook::default());
        Books::put(&mut world, none, false, ParamBook::default());
        assert!(world.contains_resource::<ParamBook>());
        let undeclared = panic::catch_unwind(AssertUnwindSafe(|| {
            Books::put(&mut world, stats, false, ModifierBook::default());
        }));
        let said = undeclared
            .unwrap_err()
            .downcast_ref::<String>()
            .cloned()
            .unwrap();
        assert!(said.ends_with("is declared by its owner"), "{said}");
    }
}
