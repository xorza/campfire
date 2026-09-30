use std::error::Error;
use std::fmt;

use crate::delegation::delegation_tag::DelegationTag;

/// Why a delegation does not let a session key sign for a player. A delegation comes from the
/// player, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegationError {
    /// The JSON is not a Nostr event.
    NotEvent,
    /// The event id is not the hash of the event.
    WrongId,
    /// The main key's signature does not hold over the event id.
    BadSignature,
    /// The event is not of the delegation kind.
    WrongKind,
    MissingTag(DelegationTag),
    RepeatedTag(DelegationTag),
    /// The tag does not hold exactly one value of its format.
    MalformedTag(DelegationTag),
    /// The delegation names another server than the header's.
    OtherServer,
    /// The delegation names another session than the header's.
    OtherSession,
}

impl fmt::Display for DelegationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DelegationError::NotEvent => f.write_str("delegation is not a Nostr event"),
            DelegationError::WrongId => f.write_str("delegation id is not the hash of the event"),
            DelegationError::BadSignature => {
                f.write_str("delegation signature does not hold under the main key")
            }
            DelegationError::WrongKind => f.write_str("delegation is not of the delegation kind"),
            DelegationError::MissingTag(tag) => {
                write!(f, "delegation lacks the {} tag", tag.name())
            }
            DelegationError::RepeatedTag(tag) => {
                write!(f, "delegation holds the {} tag more than once", tag.name())
            }
            DelegationError::MalformedTag(tag) => {
                write!(f, "delegation {} tag is malformed", tag.name())
            }
            DelegationError::OtherServer => f.write_str("delegation names another server"),
            DelegationError::OtherSession => f.write_str("delegation names another session"),
        }
    }
}

impl Error for DelegationError {}
