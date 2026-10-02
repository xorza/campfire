use std::num::NonZeroU32;

use campfire_content::Fingerprint as PackageFingerprint;
use campfire_package::{ModePackages, RELEASE, TickRange};
use campfire_protocol::secp256k1::XOnlyPublicKey;
use campfire_protocol::{Fingerprint, SeedCommitment, SessionTerms};

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
            mode: in_terms(packages.fingerprint()),
            dependencies: packages.dependency_fingerprints().map(in_terms).collect(),
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
        if !self.tick_hz.contains(terms.tick_hz) {
            return Err(TermsError::TickRate(terms.tick_hz));
        }
        Ok(())
    }

    /// An error for `terms` of another engine release than this one, which a verifier checks
    /// before it reads the packages, as another release's packages need not read in this one.
    pub fn check_release(terms: &SessionTerms) -> Result<(), TermsError> {
        if terms.release != RELEASE {
            return Err(TermsError::OtherRelease(terms.release.clone()));
        }
        Ok(())
    }
}

/// The fingerprint of a package as the session terms name it.
const fn in_terms(fingerprint: PackageFingerprint) -> Fingerprint {
    Fingerprint::new(*fingerprint.as_bytes())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use campfire_math::Ticks;
    use campfire_protocol::SeedChain;

    use super::*;

    type Change = fn(&mut SessionTerms);

    #[test]
    fn the_rules_build_terms_of_the_packages_and_refuse_terms_of_others() {
        // The 3v3 runs from 20 to 60 Hz.
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/moba/modes/3v3");
        let packages = ModePackages::from_dir(Path::new(dir)).unwrap();
        let rules = SessionRules::of(&packages);
        let key = XOnlyPublicKey::from_byte_array(&[8; 32]).unwrap();
        let commitment = SeedChain::new([7; 32], NonZeroU32::MIN).commitment();
        let inputs = InputRules {
            max_input_delay: Ticks::new(3),
            max_input_lead: Ticks::new(5),
            max_payload_len: 7,
            max_inputs_per_tick: 2,
        };
        let terms = |hz| rules.terms(key, commitment, NonZeroU32::new(hz).unwrap(), inputs);
        for hz in [19, 61] {
            assert_eq!(
                terms(hz),
                Err(TermsError::TickRate(NonZeroU32::new(hz).unwrap()))
            );
        }
        assert!(terms(20).is_ok());
        let fixed = terms(60).unwrap();
        let dependencies: Vec<_> = packages.dependency_fingerprints().map(in_terms).collect();
        assert_eq!(
            fixed,
            SessionTerms {
                server_key: key,
                tick_hz: NonZeroU32::new(60).unwrap(),
                max_input_delay: Ticks::new(3),
                max_input_lead: Ticks::new(5),
                max_payload_len: 7,
                max_inputs_per_tick: 2,
                seed_commitment: commitment,
                release: RELEASE.to_owned(),
                mode: in_terms(packages.fingerprint()),
                dependencies,
            }
        );
        assert!(fixed.dependencies.len() > 1, "the swap below reorders them");

        let changes: [(Change, TermsError); 4] = [
            (
                |terms| terms.release = "0.0.9".to_owned(),
                TermsError::OtherRelease("0.0.9".to_owned()),
            ),
            (
                |terms| terms.mode = Fingerprint::new([0; 32]),
                TermsError::OtherMode,
            ),
            (
                |terms| terms.dependencies.swap(0, 1),
                TermsError::OtherDependencies,
            ),
            (
                |terms| terms.dependencies.truncate(1),
                TermsError::OtherDependencies,
            ),
        ];
        for (change, error) in changes {
            let mut other = fixed.clone();
            change(&mut other);
            assert_eq!(rules.check(&other), Err(error));
        }
    }
}
