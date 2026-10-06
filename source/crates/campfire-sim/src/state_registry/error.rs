use thiserror::Error;

/// Why a snapshot does not restore. Snapshots are untrusted data, so every flaw is an error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SnapshotError {
    /// The format tag is missing.
    #[error("not a snapshot")]
    NotSnapshot,
    /// The bytes end inside a field.
    #[error("snapshot ends inside a field")]
    Truncated,
    /// The types differ from the registry's, by name or by order.
    #[error("snapshot types differ from the registry")]
    TypesDiffer,
    /// A value does not decode.
    #[error("snapshot value does not decode")]
    Malformed(#[source] postcard::Error),
    /// The bytes are not the canonical encoding of the state they restore: stable ids within a
    /// type do not strictly increase, or a value is encoded in a form the snapshot never writes.
    #[error("snapshot is not the canonical encoding of its state")]
    NotCanonical,
    /// A component belongs to a stable id the entity list lacks.
    #[error("snapshot component for an unknown entity")]
    UnknownEntity,
    /// The allocator would hand out an id already in use.
    #[error("snapshot allocator behind the ids in use")]
    AllocatorBehind,
    /// Bytes remain after the last field.
    #[error("snapshot has trailing bytes")]
    Trailing,
    /// A value of the type of this name breaks a rule of its type, such as an id that names
    /// nothing the match's books hold.
    #[error("snapshot value of {0} breaks its rules")]
    Invalid(&'static str),
}
