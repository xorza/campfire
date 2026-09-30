use campfire_protocol::{ConnectChallenge, SessionTerms};
use serde::{Deserialize, Serialize};

/// The server's offer to a connected client: the session's terms, which the player signs by
/// delegating, and the challenge its answer signs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub terms: SessionTerms,
    pub challenge: ConnectChallenge,
}
