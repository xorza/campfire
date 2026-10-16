use nostr::error::{Error as NostrError, ErrorKind};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::delegation::Delegation;
use crate::delegation::delegation_tag::DelegationTag;

/// Why a delegation does not let a session key sign for a player. A delegation comes from the
/// player, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum DelegationError {
    /// The JSON holds more than `Delegation::MAX_JSON` bytes.
    #[error("delegation holds more than {} bytes", Delegation::MAX_JSON)]
    TooLong,
    /// The JSON is not a Nostr event, of the kind nostr's error says.
    #[error("delegation is not a Nostr event")]
    NotEvent(#[source] EventError),
    /// The JSON is an event, but not the one spelling nostr writes of it: no signature covers
    /// its whitespace, key order, escapes or hex case, so another spelling would give one log a
    /// second fingerprint.
    #[error("delegation is not its event's canonical JSON")]
    NotCanonical,
    /// The event id is not the hash of the event.
    #[error("delegation id is not the hash of the event")]
    WrongId,
    /// The main key's signature does not hold over the event id.
    #[error("delegation signature does not hold under the main key")]
    BadSignature,
    /// The event is not of the delegation kind.
    #[error("delegation is not of the delegation kind")]
    WrongKind,
    #[error("delegation lacks the {} tag", .0.name())]
    MissingTag(DelegationTag),
    #[error("delegation holds the {} tag more than once", .0.name())]
    RepeatedTag(DelegationTag),
    /// The tag does not hold exactly one value of its format.
    #[error("delegation {} tag is malformed", .0.name())]
    MalformedTag(DelegationTag),
}

/// Why a delegation does not grant its session key this session on this server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ScopeError {
    #[error("the delegation names another server")]
    OtherServer,
    #[error("the delegation names another session")]
    OtherSession,
}

/// Why nostr reads no event from a JSON text: the kind of its error, as nostr sorts its errors,
/// which a log keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum EventError {
    #[error("the text is no well-formed event")]
    Malformed,
    #[error("the event breaks a rule of the protocol")]
    Invalid,
    #[error("the event lacks required data")]
    Missing,
    #[error("the event holds a value nostr does not support")]
    Unsupported,
    #[error("a cryptographic operation failed")]
    Crypto,
    #[error("nostr refuses the event for another cause")]
    Other,
}

impl From<NostrError> for EventError {
    fn from(error: NostrError) -> Self {
        match error.kind() {
            ErrorKind::Malformed => Self::Malformed,
            ErrorKind::Invalid => Self::Invalid,
            ErrorKind::Missing => Self::Missing,
            ErrorKind::Unsupported => Self::Unsupported,
            ErrorKind::Crypto => Self::Crypto,
            _ => Self::Other,
        }
    }
}
