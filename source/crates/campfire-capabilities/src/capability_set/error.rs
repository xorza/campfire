use std::error::Error;
use std::fmt;

use campfire_sim::Capability;

/// Why a mode's declared capabilities make no set. A manifest is untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityError {
    /// `mode` is declared, which every match has.
    DeclaresMode,
    Repeated(Capability),
    /// A capability is declared without one it builds on.
    Needs {
        capability: Capability,
        needs: Capability,
    },
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CapabilityError::DeclaresMode => f.write_str("declares mode, which every match has"),
            CapabilityError::Repeated(capability) => write!(f, "declares {capability:?} twice"),
            CapabilityError::Needs { capability, needs } => {
                write!(f, "declares {capability:?} without {needs:?}")
            }
        }
    }
}

impl Error for CapabilityError {}
