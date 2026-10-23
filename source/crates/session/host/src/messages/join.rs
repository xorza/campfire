use campfire_protocol::Signature;
use campfire_values::TeamSet;
use serde::{Deserialize, Serialize};

/// A player's answer to the server's offer: their delegation's event JSON, which the server
/// parses and checks, the session key's signature over the challenge and the channel binding of
/// the link's TLS session, and whether they play a player slot or watch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Join {
    pub delegation: String,
    pub answer: Signature,
    pub role: JoinRole,
}

/// What a joining player asks for: a player slot to play, or a spectator's place, which holds no
/// player_slot (10-replication-audiences.md, The host admits spectators, and their fog is a
/// setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JoinRole {
    Play,
    /// In a session whose spectators watch teams' views, it watches those of `teams` together,
    /// every team's for `TeamSet::ALL`; in one whose spectators see every unit, it sees them.
    Watch {
        teams: TeamSet,
    },
}
