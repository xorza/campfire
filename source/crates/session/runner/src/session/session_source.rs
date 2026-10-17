use std::sync::Arc;

use campfire_package::ModePackages;
use campfire_protocol::{ServerSeeds, SessionLog, Signer};

/// What a session's match is built from: its log, rewound to replay its ticks, the seeds its
/// server draws from, the packages of its mode, and the signer of the server that signs what the
/// session logs, none on a verifier, which only records a published log again.
#[derive(Debug)]
pub struct SessionSource<'p> {
    pub log: SessionLog,
    pub seeds: ServerSeeds,
    pub packages: &'p ModePackages,
    pub signer: Option<Arc<Signer>>,
}

/// A checkpoint a session's match resumes at: the segment it starts, and its snapshot.
#[derive(Debug, Clone, Copy)]
pub struct ResumeAt<'s> {
    pub segment: u32,
    pub snapshot: &'s [u8],
}
