use std::collections::BTreeMap;
use std::num::NonZeroU32;

use campfire_common::{DeclaredName, Version};
use campfire_mode::{OptionData, OptionPicks};
use campfire_package::ModePackages;
use campfire_protocol::{InputRules, SeatPlan, SeedCommitment, SessionTerms};
use campfire_visibility::{ModeVisibility, Visibility};
use secp256k1::XOnlyPublicKey;

use crate::session_rules::error::TermsError;

pub(crate) mod error;

/// What a session's host chooses of its terms beside what its mode's rules fix: the server's
/// key, the commitment to its seed chain, its tick rate, its input rules, how it opens each seat,
/// its visibility, and the value it picked of each of the mode's options.
#[derive(Debug, Clone)]
pub struct HostChoices<'a> {
    pub server_key: XOnlyPublicKey,
    pub seed_commitment: SeedCommitment,
    pub tick_hz: NonZeroU32,
    pub inputs: InputRules,
    pub seats: Vec<SeatPlan>,
    pub visibility: &'a Visibility,
    pub picks: &'a OptionPicks,
}

/// What a check of terms gives: their visibility and their picks, each of what the mode admits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedTerms {
    pub visibility: Visibility,
    pub picks: OptionPicks,
}

/// What a mode's packages fix of every session that plays them on this engine release: the mode
/// and its dependencies by their fingerprints, the map it loaded, the tick rates the mode runs
/// at, its visibility, of which the host sets only what the mode leaves it, and its options. A
/// server builds its terms by them, and the server, the client and the verifier check terms
/// against them, so the three agree on what a session of the packages is. It reads each fact
/// from the packages, which stay their one home, so a build copies nothing.
#[derive(Debug, Clone, Copy)]
pub struct SessionRules<'p> {
    packages: &'p ModePackages,
}

impl<'p> SessionRules<'p> {
    pub const fn of(packages: &'p ModePackages) -> Self {
        Self { packages }
    }

    /// The mode's options, of which a host picks values.
    pub const fn options(&self) -> &'p BTreeMap<DeclaredName, OptionData> {
        &self.packages.data().options
    }

    /// The mode's visibility, which a session's host may set again where the mode leaves it.
    pub const fn visibility(&self) -> &'p ModeVisibility {
        &self.packages.data().visibility
    }

    /// The terms of a session by these rules and the host's `choices`; an error when the mode
    /// does not run at their tick rate, when they plan no seat or more than the mode's teams
    /// have, and for a visibility the mode does not admit.
    pub fn terms(&self, choices: HostChoices<'_>) -> Result<SessionTerms, TermsError> {
        let HostChoices {
            server_key,
            seed_commitment,
            tick_hz,
            inputs,
            seats,
            visibility,
            picks,
        } = choices;
        let terms = SessionTerms {
            server_key,
            tick_hz,
            inputs,
            seed_commitment,
            release: Version::ENGINE,
            mode: self.packages.fingerprint(),
            map: self.packages.map_name().clone(),
            dependencies: self.packages.dependency_fingerprints().collect(),
            seats,
            visibility: visibility.encode(),
            options: picks.places().to_vec(),
        };
        self.check(&terms)?;
        Ok(terms)
    }

    /// Whether `terms` name a session by these rules: of this release, of the mode, its map and
    /// the dependencies, in their order, at a rate the mode runs at, of a seat at least and no
    /// more than the mode's teams have, and of a visibility and picks the mode admits, which it
    /// gives. The check is the one way to the terms' visibility and picks, so no reader takes
    /// them from terms unchecked.
    pub fn check(&self, terms: &SessionTerms) -> Result<CheckedTerms, TermsError> {
        Self::check_release(terms)?;
        let packages = self.packages;
        if terms.mode != packages.fingerprint() {
            return Err(TermsError::OtherMode);
        }
        if terms.map != *packages.map_name() {
            return Err(TermsError::OtherMap);
        }
        if !terms
            .dependencies
            .iter()
            .copied()
            .eq(packages.dependency_fingerprints())
        {
            return Err(TermsError::OtherDependencies);
        }
        let seats = u64::try_from(terms.seats.len()).expect("a seat count fits u64");
        let most = packages.manifest().seats();
        if seats == 0 || seats > most {
            return Err(TermsError::Seats { seats, most });
        }
        self.runs_at(terms.tick_hz)?;
        let visibility = Visibility::decode(&terms.visibility).map_err(TermsError::Visibility)?;
        if !self.visibility().admits(&visibility) {
            return Err(TermsError::OtherVisibility);
        }
        let picks = OptionPicks::checked(self.options(), &terms.options)?;
        Ok(CheckedTerms { visibility, picks })
    }

    /// An error when the mode does not run at `tick_hz`.
    pub const fn runs_at(&self, tick_hz: NonZeroU32) -> Result<(), TermsError> {
        if !self.packages.manifest().tick_hz.contains(tick_hz) {
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
