use std::num::NonZeroU32;

use campfire_common::Fingerprint;
use campfire_package::{ModePackages, RELEASE, TickRange};
use campfire_protocol::secp256k1::XOnlyPublicKey;
use campfire_protocol::{SeedCommitment, SessionTerms};

use crate::error::TermsError;
use crate::input_rules::InputRules;

/// What a mode's packages fix of every session that plays them on this engine release: the mode
/// and its dependencies by their fingerprints, and the tick rates the mode runs at. A server
/// builds its terms by them, and the server, the client and the verifier check terms against
/// them, so the three agree on what a session of the packages is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRules {
    mode: Fingerprint,
    dependencies: Box<[Fingerprint]>,
    tick_hz: TickRange,
}

impl SessionRules {
    pub fn of(packages: &ModePackages) -> SessionRules {
        SessionRules {
            mode: packages.fingerprint(),
            dependencies: packages.dependency_fingerprints().collect(),
            tick_hz: packages.manifest().tick_hz,
        }
    }

    /// The terms of a session by these rules on the server `server_key`, whose seed chain
    /// `seed_commitment` commits to, at `tick_hz`, its inputs within `inputs`; an error when the
    /// mode does not run at `tick_hz`.
    pub fn terms(
        &self,
        server_key: XOnlyPublicKey,
        seed_commitment: SeedCommitment,
        tick_hz: NonZeroU32,
        inputs: InputRules,
    ) -> Result<SessionTerms, TermsError> {
        let terms = SessionTerms {
            server_key,
            tick_hz,
            max_input_delay: inputs.max_input_delay,
            max_input_lead: inputs.max_input_lead,
            max_payload_len: inputs.max_payload_len,
            max_inputs_per_tick: inputs.max_inputs_per_tick,
            seed_commitment,
            release: RELEASE.to_owned(),
            mode: self.mode,
            dependencies: self.dependencies.to_vec(),
        };
        self.check(&terms)?;
        Ok(terms)
    }

    /// Whether `terms` name a session by these rules: of this release, of the mode and the
    /// dependencies, in their order, and at a rate the mode runs at.
    pub fn check(&self, terms: &SessionTerms) -> Result<(), TermsError> {
        SessionRules::check_release(terms)?;
        if terms.mode != self.mode {
            return Err(TermsError::OtherMode);
        }
        if *terms.dependencies != *self.dependencies {
            return Err(TermsError::OtherDependencies);
        }
        self.runs_at(terms.tick_hz)
    }

    /// An error when the mode does not run at `tick_hz`.
    pub const fn runs_at(&self, tick_hz: NonZeroU32) -> Result<(), TermsError> {
        if !self.tick_hz.contains(tick_hz) {
            return Err(TermsError::TickRate(tick_hz));
        }
        Ok(())
    }

    /// An error for `terms` of another engine release than this one, which a verifier checks
    /// before it reads the packages, as another release's packages need not read in this one.
    pub(crate) fn check_release(terms: &SessionTerms) -> Result<(), TermsError> {
        if terms.release != RELEASE {
            return Err(TermsError::OtherRelease(terms.release.clone()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
