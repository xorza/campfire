use campfire_math::Num;
use campfire_script::ScriptId;

use crate::abilities::ability_book::AbilityId;
use crate::mode::map_data::MapData;
use crate::mode::mode_data::ModeData;
use crate::mode::team_manifest::TeamManifest;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::walker::Walker;
use crate::stats::modifier_book::ModifierId;
use crate::stats::stats_data::StatsData;
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
    /// Each kind of unit that walks, by its layer and its body's radius: the clearances of the
    /// map's pathing grid.
    pub walkers: Vec<Walker>,
    /// The manifest's move speed cap, in meters a second.
    pub max_move_speed: Num,
    /// The places of the mode's stats in the order the stats refresh computes them, which the
    /// mode's stat graph gives.
    pub stat_order: Vec<u16>,
}

/// A unit type the mode spawns, loaded, with its kit: a type of the mode's `units.toml`, or a
/// avatar's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTypeSetup {
    pub unit_type: UnitType,
    pub kit: UnitKit,
    /// Its `stats` section, empty when it has none.
    pub stats: StatsData,
}

/// An avatar the mode depends on, loaded: its id is its package's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarSetup {
    pub id: String,
    pub unit_type: UnitType,
    /// Its abilities, in the order of its slots.
    pub abilities: Vec<AbilityId>,
    /// The modifier it carries from its spawn on.
    pub passive: Option<ModifierId>,
}

/// An entry of the mode's loadout packages, loaded, by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadoutSetup {
    pub id: String,
    pub ability: AbilityId,
}
