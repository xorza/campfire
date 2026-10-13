use campfire_state_types::DataKind;
use thiserror::Error;

/// Why the visibility a session's terms hold does not read: terms come from a server or a
/// published log, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum VisibilityDecodeError {
    /// The bytes end before the last setting, or go on after it.
    #[error("the visibility holds {0} bytes")]
    Length(usize),
    /// A kind's byte names a recipient that no mode sets for it, or another bit.
    #[error("the audience of {} does not read", .0.name())]
    Audience(DataKind),
    #[error("the spectators' fog does not read")]
    Spectators,
    #[error("what a player out of play sees does not read")]
    Out,
}
