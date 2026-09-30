use campfire_script::ScriptId;

use crate::abilities::ability_book::AbilityId;
use crate::abilities::resource_pool::ResourcePool;
use crate::mode::map_data::MapData;
use crate::mode::mode_data::ModeData;
use crate::mode::team_manifest::TeamManifest;
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
    /// Every unit type it spawns, avatars' included.
    pub unit_types: Vec<UnitTypeSetup>,
    pub avatars: Vec<AvatarSetup>,
    pub loadout: Vec<LoadoutSetup>,
}

/// A unit type the mode spawns, loaded, with its kit: a type of the mode's `units.toml`, or a
/// avatar's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTypeSetup {
    pub unit_type: UnitType,
    pub kit: UnitKit,
}

/// An avatar the mode depends on, loaded: its id is its package's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarSetup {
    pub id: String,
    pub unit_type: UnitType,
    /// Its abilities, in the order of its slots.
    pub abilities: Vec<AbilityId>,
    pub resource: Option<ResourcePool>,
}

/// An entry of the mode's loadout packages, loaded, by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadoutSetup {
    pub id: String,
    pub ability: AbilityId,
}
