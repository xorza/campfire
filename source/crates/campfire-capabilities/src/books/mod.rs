//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

use crate::actions::action_book::ActionBook;
use crate::actions::actions_column::ActionsColumn;
use crate::actions::effect_lists::EffectLists;
use crate::areas::area_spec::AreaSpec;
use crate::books::book_builder::BookBuilder;
use crate::books::book_input::BookInput;
use crate::books::error::BookError;
use crate::mode::mode_books::ModeBooks;
use crate::mode::mode_map::ModeMap;
use crate::mode::mode_units::ModeUnits;
use crate::navigation::walker::Walker;
use crate::orders::ai::Ai;
use crate::production::production_data::ProductionData;
use crate::progression::track_book::TrackBook;
use crate::progression::tracks_column::TracksColumn;
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::scripts::ctx::Ctx;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::param_book::{ParamBook, ParamTables};
use crate::stats::stats_call::StatsCall;
use crate::stats::stats_column::StatsColumn;
use crate::units::by_type::ByType;
use crate::units::predicting::Predicting;
use crate::units::script_view::View;
use crate::units::unit_state_column::UnitStateColumn;
use crate::units::unit_types::UnitTypes;

pub(crate) mod book_builder;
pub(crate) mod book_input;
pub(crate) mod error;
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

/// What the builder loads package by package: the unit types and tags, the tracks, the
/// modifiers and actions with their params and effect lists, the AIs, the projectile and area
/// specs, and what the mode's book takes from them.
#[derive(Debug, Default)]
pub(crate) struct BookParts {
    types: UnitTypes,
    tracks: Option<TrackBook>,
    modifiers: ModifierBook,
    actions: ActionBook,
    /// Every action's and modifier's params.
    params: ParamTables,
    effects: EffectLists,
    ais: ByType<Ai>,
    projectiles: ByType<ProjectileSpec>,
    areas: ByType<AreaSpec>,
    producers: ByType<ProductionData>,
    units: ModeUnits,
}

/// What the mode's install takes from the books: its unit types, avatars and loadout, and the
/// books of its own rules.
#[derive(Debug)]
pub struct ModeInputs {
    pub units: ModeUnits,
    pub books: ModeBooks,
}

impl Books {
    /// The books of `input`, which the package load checked; an error for what the check does
    /// not see and the books cannot hold.
    pub fn build(input: &BookInput<'_>) -> Result<Books, BookError> {
        BookBuilder::new(input).build()
    }

    /// Puts the books in `world`, a match whose capabilities are installed and whose scripts are
    /// compiled, each in the resource of the capability that reads it, and the view's and, with
    /// scripts, the frame's copies; what the mode's book takes stays.
    pub fn install(self, world: &mut World) -> ModeInputs {
        let Books { parts, mode } = self;
        let view = world.non_send::<View>().clone();
        let states = parts.types.state_book();
        UnitStateColumn::share(&view, states.clone());
        world.insert_resource(states);
        view.set_types(parts.types);
        StatsColumn::share_modifiers(&view, parts.modifiers.clone());
        let params = ParamBook::new(parts.params);
        if let Some(ctx) = world.get_non_send::<Ctx>() {
            StatsCall::share_params(&mut ctx.frame(), params.clone());
        }
        world.insert_resource(params);
        if let Some(tracks) = parts.tracks {
            TracksColumn::share(&view, tracks.clone());
            world.insert_resource(tracks);
        }
        ActionsColumn::share(&view, parts.actions.clone());
        replace(world, parts.modifiers);
        replace(world, parts.actions);
        replace(world, parts.effects);
        replace(world, parts.ais);
        replace(world, parts.projectiles);
        replace(world, parts.areas);
        replace(world, parts.producers);
        ModeInputs {
            units: parts.units,
            books: mode,
        }
    }

    /// Puts the books in `world`, a client's, whose capabilities are installed with no scripts,
    /// and the part of the mode no script runs, as a match's mode install puts them: the books
    /// of its own rules, and its map's ground, its pathing grid for the kinds of `walkers`. The
    /// client then predicts its units by the rules the server runs: it starts their actions, and
    /// runs none of their effects.
    pub fn install_prediction(self, world: &mut World, walkers: Vec<Walker>) {
        let ModeInputs { books, .. } = self.install(world);
        let ModeMap { ground, .. } = books.install(world);
        ground.install(world, walkers);
        world.insert_resource(Predicting);
    }
}

/// Puts `book` in place of the resource of its type in `world`, which the capability that reads
/// it installed; none when the match does not have that capability, whose book is then empty.
fn replace<T: Resource>(world: &mut World, book: T) {
    if world.contains_resource::<T>() {
        world.insert_resource(book);
    }
}
