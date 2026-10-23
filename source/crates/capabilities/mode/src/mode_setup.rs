use campfire_script::ModeScripts;

use crate::host_picks::HostPicks;
use crate::mode_data::ModeData;
use crate::team_manifest::TeamManifest;

/// What a match of a mode needs from its packages, beside its books: the input of `Mode::load`,
/// with what its books give the mode.
#[derive(Debug)]
pub struct ModeSetup<'a> {
    /// The scripts its list names, compiled.
    pub scripts: ModeScripts,
    pub data: &'a ModeData,
    /// What its session's host picked of `data`'s options and of each player slot's choices.
    pub picks: &'a HostPicks,
    /// The playing teams, in the manifest's order; their slots in that order make the player
    /// slots.
    pub teams: &'a [TeamManifest],
    /// The players of the session.
    pub players: u32,
}
