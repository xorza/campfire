use campfire_protocol::Signature;
use serde::{Deserialize, Serialize};

/// A player's answer to the server's offer: their delegation's event JSON, which the server
/// parses and checks, the session key's signature over the challenge and the channel binding of
/// the link's TLS session, and whether they play a slot or watch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Join {
    pub delegation: String,
    pub answer: Signature,
    pub role: JoinRole,
}

/// What a joining player asks for: a slot to play, or a spectator's place, which holds no slot
/// (14-replication-audiences.md, The host admits spectators, and their fog is a setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JoinRole {
    Play,
    Watch,
}
