use campfire_script::ScriptId;

use crate::abilities::ability_book::AbilityId;
use crate::abilities::resource_pool::ResourcePool;
use crate::files::manifest::TeamManifest;
use crate::files::map_data::MapData;
use crate::files::mode_data::ModeData;
use crate::mode::unit_kit::UnitKit;
use crate::units::unit_type::UnitType;

/// What a match of a mode needs, from its packages, with its unit types, abilities and AI loaded
/// into the match: the input of `Mode::install`.
#[derive(Debug)]
pub struct ModeSetup<'a> {
    /// The mode script, compiled.
    pub script: ScriptId,
    pub data: &'a ModeData,
    pub map: &'a MapData,
    /// The playing teams, in the manifest's order; their slots in that order make the player
    /// slots.
    pub teams: &'a [TeamManifest],
    /// The players of the session.
    pub players: u32,
    /// Every unit type it spawns, heroes' included.
    pub unit_types: Vec<UnitTypeSetup>,
    pub heroes: Vec<HeroSetup>,
    pub spells: Vec<SpellSetup>,
}

/// A unit type the mode spawns, loaded, with its kit: a type of the mode's `units.toml`, or a
/// hero's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTypeSetup {
    pub unit_type: UnitType,
    pub kit: UnitKit,
}

/// A hero the mode depends on, loaded: its id is its package's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroSetup {
    pub id: String,
    pub unit_type: UnitType,
    /// Its abilities, in the order of its slots.
    pub abilities: Vec<AbilityId>,
    pub resource: Option<ResourcePool>,
}

/// A spell of the mode's spells packages, loaded, by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellSetup {
    pub id: String,
    pub ability: AbilityId,
}
