use thiserror::Error;

use crate::delegation::Delegation;
use crate::delegation::delegation_tag::DelegationTag;

/// Why a delegation does not let a session key sign for a player. A delegation comes from the
/// player, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DelegationError {
    /// The JSON holds more than `Delegation::MAX_JSON` bytes.
    #[error("delegation holds more than {} bytes", Delegation::MAX_JSON)]
    TooLong,
    /// The JSON is not a Nostr event.
    #[error("delegation is not a Nostr event")]
    NotEvent,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ScopeError {
    #[error("the delegation names another server")]
    OtherServer,
    #[error("the delegation names another session")]
    OtherSession,
}
