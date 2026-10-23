use campfire_common::PlayerSlot;
use campfire_visibility::Recipients;

use crate::team::Team;

/// How a player views the match: as the player of `player_slot`, of `team`; `out` while it is out
/// of play in a session whose players out of play see the other teams as a spectator does. The one
/// rule of which recipient it is of each entity, which the server's data filters and a bot's script
/// view both take (10-replication-audiences.md, Four recipients; 10-replication-audiences.md, A
/// player out of play sees what its team sees, or what a spectator sees).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerView {
    pub player_slot: PlayerSlot,
    pub team: Team,
    pub out: bool,
}

impl PlayerView {
    /// The recipient it is of an entity `owner` owns, of `team`: none for the owner, who
    /// receives every sent kind; an ally of its own team, or of an entity of no team, as the
    /// match's; an enemy of another, or a spectator of it while out of play.
    pub fn recipient(self, owner: Option<PlayerSlot>, team: Option<Team>) -> Recipients {
        if Some(self.player_slot) == owner {
            return Recipients::NONE;
        }
        match team {
            Some(team) if team != self.team && self.out => Recipients::SPECTATORS,
            Some(team) if team != self.team => Recipients::ENEMIES,
            _ => Recipients::ALLIES,
        }
    }
}
