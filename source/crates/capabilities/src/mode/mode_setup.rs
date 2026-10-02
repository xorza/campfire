use campfire_script::ScriptId;

use crate::actions::action_book::ActionId;
use crate::actions::slot_kind::SlotKind;
use crate::mode::mode_data::ModeData;
use crate::mode::team_manifest::TeamManifest;
use crate::mode::unit_kit::UnitKit;
use crate::navigation::walker::Walker;
use crate::stats::modifier_book::ModifierId;
use crate::units::unit_type::UnitType;

/// What a match of a mode needs, from its packages, with its unit types, abilities and AI loaded
/// into the match: the input of `Mode::install`.
#[derive(Debug)]
pub struct ModeSetup<'a> {
    /// The mode script, compiled.
    pub script: ScriptId,
    pub data: &'a ModeData,
    /// The playing teams, in the manifest's order; their slots in that order make the player
    /// slots.
    pub teams: &'a [TeamManifest],
    /// The players of the session.
    pub players: u32,
    /// Every unit type it spawns, avatars' included.
    pub unit_types: Vec<UnitTypeSetup>,
    /// The ids of the avatars the mode depends on, in order: each its package's name, and its
    /// unit type's.
    pub avatars: Vec<String>,
    pub loadout: Vec<LoadoutSetup>,
    /// Each kind of unit that walks, by its layer and its body's radius: the clearances of the
    /// map's pathing grid.
    pub walkers: Vec<Walker>,
}

/// A unit type the mode spawns, loaded, with its kit: a type of the mode's `units.toml`, or an
/// avatar's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTypeSetup {
    pub unit_type: UnitType,
    pub kit: UnitKit,
    /// Its actions, kind after kind in the mode's order.
    pub actions: Vec<SlotAction>,
    /// The modifier it holds from its spawn on, from itself.
    pub passive: Option<ModifierId>,
}

/// An action in a slot kind of a unit type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotAction {
    pub kind: SlotKind,
    pub ability: ActionId,
}

/// An entry of the mode's loadout packages, loaded, by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadoutSetup {
    pub id: String,
    pub ability: ActionId,
}
