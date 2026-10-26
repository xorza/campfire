use campfire_declarations::BookError;
use campfire_sim::Capability;
use thiserror::Error;

/// Why a mode's declared capabilities make no set. A manifest is untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CapabilityError {
    /// `mode` is declared, which every match has.
    #[error("declares mode, which every match has")]
    DeclaresMode,
    #[error("declares {0:?} twice")]
    Repeated(Capability),
    /// A capability is declared without one it builds on.
    #[error("declares {capability:?} without {needs:?}")]
    Needs {
        capability: Capability,
        needs: Capability,
    },
}

/// Why a checked build of a match's books gave none: its books refused the data, or the caller's
/// check between the rules and the books failed.
#[derive(Debug)]
pub enum CheckedBooksError<E> {
    Book(BookError),
    Check(E),
}
