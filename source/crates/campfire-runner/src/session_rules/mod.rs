use std::num::NonZeroU32;

use campfire_common::{Fingerprint, MapName};
use campfire_package::{ModePackages, RELEASE, TickRange};
use campfire_protocol::secp256k1::XOnlyPublicKey;
use campfire_protocol::{SeedCommitment, SessionTerms, SlotPlan};

use crate::input_rules::InputRules;
use crate::session_rules::error::TermsError;

pub(crate) mod error;

/// What a mode's packages fix of every session that plays them on this engine release: the mode
/// and its dependencies by their fingerprints, the map it loaded, and the tick rates the mode runs
/// at. A server builds its terms by them, and the server, the client and the verifier check terms
/// against them, so the three agree on what a session of the packages is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRules {
    mode: Fingerprint,
    map: MapName,
    dependencies: Box<[Fingerprint]>,
    tick_hz: TickRange,
    /// The player slots the mode's teams have.
    slots: u64,
}

impl SessionRules {
    pub fn of(packages: &ModePackages) -> SessionRules {
        SessionRules {
            mode: packages.fingerprint(),
            map: packages.map_name().clone(),
            dependencies: packages.dependency_fingerprints().collect(),
            tick_hz: packages.manifest().tick_hz,
            slots: packages.manifest().slots(),
        }
    }

    /// The terms of a session by these rules on the server `server_key`, whose seed chain
    /// `seed_commitment` commits to, at `tick_hz`, its inputs within `inputs`, its slots opened
    /// as `slots` plans them; an error when the mode does not run at `tick_hz`, or when `slots`
    /// plans none or more than the mode's teams have.
    pub fn terms(
        &self,
        server_key: XOnlyPublicKey,
        seed_commitment: SeedCommitment,
        tick_hz: NonZeroU32,
        inputs: InputRules,
        slots: Vec<SlotPlan>,
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
            map: self.map.clone(),
            dependencies: self.dependencies.to_vec(),
            slots,
        };
        self.check(&terms)?;
        Ok(terms)
    }

    /// Whether `terms` name a session by these rules: of this release, of the mode, its map and
    /// the dependencies, in their order, at a rate the mode runs at, and of a slot at least and no
    /// more than the mode's teams have.
    pub fn check(&self, terms: &SessionTerms) -> Result<(), TermsError> {
        SessionRules::check_release(terms)?;
        if terms.mode != self.mode {
            return Err(TermsError::OtherMode);
        }
        if terms.map != self.map {
            return Err(TermsError::OtherMap);
        }
        if *terms.dependencies != *self.dependencies {
            return Err(TermsError::OtherDependencies);
        }
        let slots = u64::try_from(terms.slots.len()).expect("a slot count fits u64");
        if slots == 0 || slots > self.slots {
            return Err(TermsError::Slots {
                slots,
                most: self.slots,
            });
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
