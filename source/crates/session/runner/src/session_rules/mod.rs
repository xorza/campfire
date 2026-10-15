use std::num::NonZeroU32;

use campfire_common::{Fingerprint, MapName, Version};
use campfire_package::{ModePackages, TickRange};
use campfire_protocol::{InputRules, SeedCommitment, SessionTerms, SlotPlan};
use campfire_visibility::{ModeVisibility, Visibility};
use secp256k1::XOnlyPublicKey;

use crate::session_rules::error::TermsError;

pub(crate) mod error;

/// What a mode's packages fix of every session that plays them on this engine release: the mode
/// and its dependencies by their fingerprints, the map it loaded, the tick rates the mode runs
/// at, and its visibility, of which the host sets only what the mode leaves it. A server builds
/// its terms by them, and the server, the client and the verifier check terms against them, so
/// the three agree on what a session of the packages is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRules {
    mode: Fingerprint,
    map: MapName,
    dependencies: Box<[Fingerprint]>,
    tick_hz: TickRange,
    /// The player slots the mode's teams have.
    slots: u64,
    visibility: ModeVisibility,
}

impl SessionRules {
    pub fn of(packages: &ModePackages) -> SessionRules {
        SessionRules {
            mode: packages.fingerprint(),
            map: packages.map_name().clone(),
            dependencies: packages.dependency_fingerprints().collect(),
            tick_hz: packages.manifest().tick_hz,
            slots: packages.manifest().slots(),
            visibility: packages.data().visibility.clone(),
        }
    }

    /// The mode's visibility, which a session's host may set again where the mode leaves it.
    pub const fn visibility(&self) -> &ModeVisibility {
        &self.visibility
    }

    /// The terms of a session by these rules on the server `server_key`, whose seed chain
    /// `seed_commitment` commits to, at `tick_hz`, its inputs within `rules`, its slots opened
    /// as `slots` plans them, of `visibility`; an error when the mode does not run at `tick_hz`,
    /// when `slots` plans none or more than the mode's teams have, and for a visibility the mode
    /// does not admit.
    pub fn terms(
        &self,
        server_key: XOnlyPublicKey,
        seed_commitment: SeedCommitment,
        tick_hz: NonZeroU32,
        rules: InputRules,
        slots: impl IntoIterator<Item = SlotPlan>,
        visibility: &Visibility,
    ) -> Result<SessionTerms, TermsError> {
        let terms = SessionTerms {
            server_key,
            tick_hz,
            inputs: rules,
            seed_commitment,
            release: Version::ENGINE,
            mode: self.mode,
            map: self.map.clone(),
            dependencies: self.dependencies.to_vec(),
            slots: slots.into_iter().collect(),
            visibility: visibility.encode(),
        };
        self.check(&terms)?;
        Ok(terms)
    }

    /// Whether `terms` name a session by these rules: of this release, of the mode, its map and
    /// the dependencies, in their order, at a rate the mode runs at, of a slot at least and no
    /// more than the mode's teams have, and of a visibility the mode admits, which it gives. The
    /// check is the one way to the terms' visibility, so no reader takes it from terms unchecked.
    pub fn check(&self, terms: &SessionTerms) -> Result<Visibility, TermsError> {
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
        self.runs_at(terms.tick_hz)?;
        let visibility = Visibility::decode(&terms.visibility).map_err(TermsError::Visibility)?;
        if !self.visibility.admits(&visibility) {
            return Err(TermsError::OtherVisibility);
        }
        Ok(visibility)
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
    pub fn check_release(terms: &SessionTerms) -> Result<(), TermsError> {
        if terms.release != Version::ENGINE {
            return Err(TermsError::OtherRelease(terms.release));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
