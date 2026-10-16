use campfire_protocol::{ConnectChallenge, SessionTerms};
use serde::{Deserialize, Serialize};

use crate::session_times::SessionTimes;

/// The server's offer to a connected client: the session's terms, which the player signs by
/// delegating, how long the server waits for a player whose link fails, and the challenge its
/// answer signs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Offer {
    pub terms: SessionTerms,
    pub times: SessionTimes,
    pub challenge: ConnectChallenge,
}
