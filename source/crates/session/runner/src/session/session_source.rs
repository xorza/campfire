use std::sync::Arc;

use campfire_package::ModePackages;
use campfire_protocol::{ServerSeeds, SessionLog, Signer};

use crate::session_rules::error::TermsError;
use crate::session_rules::{CheckedTerms, SessionRules};

/// What a session's match is built from: its log, whose sealed ticks the session replays, the seeds
/// its server draws from, the packages of its mode, the log's terms as the mode's rules checked
/// them, once for the session, and the signer of the server that signs what the session logs, none
/// on a verifier, which only records a published log again.
#[derive(Debug)]
pub struct SessionSource<'p> {
    pub log: SessionLog,
    pub seeds: ServerSeeds,
    pub packages: &'p ModePackages,
    pub checked: CheckedTerms,
    pub signer: Option<Arc<Signer>>,
}

/// What `SessionSource::check` takes: the source but the check of its terms.
#[derive(Debug)]
pub struct UncheckedSource<'p> {
    pub log: SessionLog,
    pub seeds: ServerSeeds,
    pub packages: &'p ModePackages,
    pub signer: Option<Arc<Signer>>,
}

impl<'p> SessionSource<'p> {
    /// The source of `unchecked`, its log's terms checked by its mode's rules; an error for terms
    /// the rules do not admit.
    pub fn check(unchecked: UncheckedSource<'p>) -> Result<Self, TermsError> {
        let UncheckedSource {
            log,
            seeds,
            packages,
            signer,
        } = unchecked;
        let checked = SessionRules::of(packages).check(&log.header().terms)?;
        Ok(Self {
            log,
            seeds,
            packages,
            checked,
            signer,
        })
    }
}

/// A checkpoint a session's match resumes at: the segment it starts, and its snapshot.
#[derive(Debug, Clone, Copy)]
pub struct ResumeAt<'s> {
    pub segment: u32,
    pub snapshot: &'s [u8],
}
