use campfire_protocol::Signature;
use serde::{Deserialize, Serialize};

/// A player's answer to the server's offer: their delegation's event JSON, which the server
/// parses and checks, and the session key's signature over the challenge and the certificate hash
/// the client verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Join {
    pub delegation: String,
    pub answer: Signature,
}
