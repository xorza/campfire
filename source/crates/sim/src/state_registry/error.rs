use std::error::Error;
use std::fmt;

/// Why a snapshot does not restore. Snapshots are untrusted data, so every flaw is an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    /// The format tag is missing.
    NotSnapshot,
    /// The bytes end inside a field.
    Truncated,
    /// The types differ from the registry's, by name or by order.
    TypesDiffer,
    /// A value does not decode.
    Malformed(postcard::Error),
    /// The bytes are not the canonical encoding of the state they restore: stable ids within a
    /// type do not strictly increase, or a value is encoded in a form the snapshot never writes.
    NotCanonical,
    /// A component belongs to a stable id the entity list lacks.
    UnknownEntity,
    /// The allocator would hand out an id already in use.
    AllocatorBehind,
    /// Bytes remain after the last field.
    Trailing,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SnapshotError::NotSnapshot => "not a snapshot",
            SnapshotError::Truncated => "snapshot ends inside a field",
            SnapshotError::TypesDiffer => "snapshot types differ from the registry",
            SnapshotError::Malformed(_) => "snapshot value does not decode",
            SnapshotError::NotCanonical => "snapshot is not the canonical encoding of its state",
            SnapshotError::UnknownEntity => "snapshot component for an unknown entity",
            SnapshotError::AllocatorBehind => "snapshot allocator behind the ids in use",
            SnapshotError::Trailing => "snapshot has trailing bytes",
        })
    }
}

impl Error for SnapshotError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SnapshotError::Malformed(error) => Some(error),
            _ => None,
        }
    }
}
