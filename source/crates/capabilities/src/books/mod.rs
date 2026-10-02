//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

use crate::abilities::effect_lists::EffectLists;
use crate::actions::action_book::{ActionBook, ActionId, Delivery};
use crate::areas::area_spec::AreaSpec;
use crate::books::book_builder::BookBuilder;
use crate::books::book_input::BookInput;
use crate::books::error::BookError;
use crate::mode::mode_books::ModeBooks;
use crate::mode::mode_setup::{LoadoutSetup, UnitTypeSetup};
use crate::orders::ai::Ai;
use crate::progression::track_book::TrackBook;
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::scripts::ctx::Ctx;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::param_book::{ParamBook, ParamTables};
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;

pub(crate) mod book_builder;
pub(crate) mod book_input;
pub(crate) mod error;
pub(crate) mod package_content;
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
    /// Each action's name and how it delivers, by action id.
    abilities: Vec<AbilityName>,
    effects: EffectLists,
    ais: ByType<Ai>,
    projectiles: ByType<ProjectileSpec>,
    areas: ByType<AreaSpec>,
    /// The projectile types that home.
    homing: Vec<UnitType>,
    /// Each train's and delivery's unit type.
    spawns: Vec<Spawn>,
    units: ModeUnits,
}

/// The mode's unit types that stand, its avatars' among them, its avatars by their packages'
/// names, and its loadout's entries.
#[derive(Debug, Default)]
pub struct ModeUnits {
    pub unit_types: Vec<UnitTypeSetup>,
    pub avatars: Vec<String>,
    pub loadout: Vec<LoadoutSetup>,
}

/// What the mode's install takes from the books: its unit types, avatars and loadout, and the
/// books of its own rules.
#[derive(Debug)]
pub struct ModeInputs {
    pub units: ModeUnits,
    pub books: ModeBooks,
}

/// An action's name, and how it delivers.
#[derive(Debug)]
struct AbilityName {
    name: Box<str>,
    delivery: Option<Delivery>,
}

/// An action that spawns a unit type, and the type.
#[derive(Debug, Clone, Copy)]
struct Spawn {
    action: ActionId,
    unit_type: UnitType,
}

impl Books {
    /// The books of `input`, which the package load checked; an error for what the check does
    /// not see and the books cannot hold.
    pub fn build(input: &BookInput<'_>) -> Result<Books, BookError> {
        let mut parts = BookBuilder::new(input).build()?;
        let mode = ModeBooks::build(
            input.data,
            &parts.units.unit_types,
            &mut parts.types,
            input.rate,
            input.max_move_speed.get(),
            input.stat_order.clone(),
        );
        Ok(Books { parts, mode })
    }

    /// Puts the books in `world`, a match whose capabilities are installed and whose scripts are
    /// compiled, each in the resource of the capability that reads it, and the view's and the
    /// frame's copies; what the mode's book takes stays.
    pub fn install(self, world: &mut World) -> ModeInputs {
        let Books { parts, mode } = self;
        let ctx = world.non_send::<Ctx>().clone();
        let view = ctx.view();
        view.set_types(parts.types);
        for info in parts.modifiers.infos() {
            view.add_modifier(info);
        }
        let params = ParamBook::new(parts.params);
        ctx.frame().set_params(params.clone());
        world.insert_resource(params);
        if let Some(tracks) = parts.tracks {
            view.set_track_names(tracks.names());
            world.insert_resource(tracks);
        }
        for ability in &parts.abilities {
            view.add_ability(&ability.name, ability.delivery);
        }
        for spawn in &parts.spawns {
            view.bind_spawn(spawn.action, spawn.unit_type);
        }
        for &unit_type in &parts.homing {
            view.set_homing(unit_type);
        }
        replace(world, parts.modifiers);
        replace(world, parts.actions);
        replace(world, parts.effects);
        replace(world, parts.ais);
        replace(world, parts.projectiles);
        replace(world, parts.areas);
        ModeInputs {
            units: parts.units,
            books: mode,
        }
    }
}

/// Puts `book` in place of the resource of its type in `world`, which the capability that reads
/// it installed; none when the match does not have that capability, whose book is then empty.
fn replace<T: Resource>(world: &mut World, book: T) {
    if world.contains_resource::<T>() {
        world.insert_resource(book);
    }
}
