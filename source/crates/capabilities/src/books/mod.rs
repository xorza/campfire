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
use crate::mode::mode_setup::{LoadoutSetup, UnitTypeSetup};
use crate::orders::ai::Ai;
use crate::progression::track_book::TrackBook;
use crate::projectiles::projectile_spec::ProjectileSpec;
use crate::scripts::ctx::Ctx;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::param_table::ParamTable;
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;

pub(crate) mod book_builder;
pub(crate) mod book_input;
pub(crate) mod error;
pub(crate) mod package_content;
pub(crate) mod unit_type_file;

/// The books of a match, built once from its packages at its tick rate: its unit types and
/// tags, its tracks, its modifiers and actions with their params and effect lists, its AIs, and
/// its projectile and area specs; and what the mode's book takes from them.
#[derive(Debug, Default)]
pub struct Books {
    types: UnitTypes,
    tracks: Option<TrackBook>,
    modifiers: ModifierBook,
    modifier_params: ParamTable,
    actions: ActionBook,
    action_params: ParamTable,
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

/// What the mode's book takes from the books: the mode's unit types that stand, its avatars'
/// among them, its avatars by their packages' names, and its loadout's entries.
#[derive(Debug, Default)]
pub struct ModeUnits {
    pub unit_types: Vec<UnitTypeSetup>,
    pub avatars: Vec<String>,
    pub loadout: Vec<LoadoutSetup>,
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
        BookBuilder::new(input).build()
    }

    /// Puts the books in `world`, a match whose capabilities are installed and whose scripts are
    /// compiled, each in the resource of the capability that reads it, and the view's and the
    /// frame's copies; what the mode's book takes stays.
    pub fn install(self, world: &mut World) -> ModeUnits {
        let ctx = world.non_send::<Ctx>().clone();
        let view = ctx.view();
        view.set_types(self.types);
        for info in self.modifiers.infos() {
            view.add_modifier(info);
        }
        {
            let mut frame = ctx.frame();
            frame.set_params(self.action_params, self.modifier_params);
        }
        if let Some(tracks) = self.tracks {
            view.set_track_names(tracks.names());
            world.insert_resource(tracks);
        }
        for ability in &self.abilities {
            view.add_ability(&ability.name, ability.delivery);
        }
        for spawn in &self.spawns {
            view.bind_spawn(spawn.action, spawn.unit_type);
        }
        for &unit_type in &self.homing {
            view.set_homing(unit_type);
        }
        replace(world, self.modifiers);
        replace(world, self.actions);
        replace(world, self.effects);
        replace(world, self.ais);
        replace(world, self.projectiles);
        replace(world, self.areas);
        self.units
    }
}

/// Puts `book` in place of the resource of its type in `world`, which the capability that reads
/// it installed; none when the match does not have that capability, whose book is then empty.
fn replace<T: Resource>(world: &mut World, book: T) {
    if world.contains_resource::<T>() {
        world.insert_resource(book);
    }
}
