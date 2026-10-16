use campfire_script::ScriptId;

use crate::mode_data::ModeData;
use crate::mode_units::ModeUnits;
use crate::option_picks::OptionPicks;
use crate::team_manifest::TeamManifest;

/// What a match of a mode needs, from its packages, with its unit types, abilities and AI loaded
/// into the match: the input of `Mode::load`.
#[derive(Debug)]
pub struct ModeSetup<'a> {
    /// The mode script, compiled.
    pub script: ScriptId,
    pub data: &'a ModeData,
    /// The value its session's host picked of each of `data`'s options.
    pub picks: &'a OptionPicks,
    /// The playing teams, in the manifest's order; their slots in that order make the player
    /// slots.
    pub teams: &'a [TeamManifest],
    /// The players of the session.
    pub players: u32,
    pub units: ModeUnits,
}
