use std::num::NonZeroU32;

use campfire_capabilities::CapabilitySet;
use campfire_package::{ModePackages, RELEASE};
use campfire_protocol::{Fingerprint, SessionTerms};
use campfire_runner::Session;

use crate::error::TermsMismatch;

/// The session a client can play: the engine release it runs, and the mode it holds, at the mode's
/// default tick rate, with the capabilities the mode declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientMode {
    pub tick_hz: NonZeroU32,
    pub mode: Fingerprint,
    pub dependencies: Vec<Fingerprint>,
    pub capabilities: CapabilitySet,
}

impl ClientMode {
    pub fn of(packages: &ModePackages) -> ClientMode {
        ClientMode {
            tick_hz: packages.manifest().tick_hz.default(),
            mode: Session::mode_in_terms(packages),
            dependencies: Session::dependencies_in_terms(packages),
            capabilities: packages.manifest().capabilities,
        }
    }

    /// Whether `terms` name a session of this mode, release and rate.
    pub(crate) fn fits(&self, terms: &SessionTerms) -> Result<(), TermsMismatch> {
        if terms.release != RELEASE {
            return Err(TermsMismatch::OtherRelease);
        }
        if terms.mode != self.mode {
            return Err(TermsMismatch::OtherMode);
        }
        if terms.dependencies != self.dependencies {
            return Err(TermsMismatch::OtherDependencies);
        }
        if terms.tick_hz != self.tick_hz {
            return Err(TermsMismatch::OtherTickRate);
        }
        Ok(())
    }
}
