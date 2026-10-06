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
