use campfire_package::ModePackages;
use campfire_protocol::{ServerSeeds, SessionLog, Signer};

/// What a session's match is built from: its log, rewound to replay its ticks, the seeds its
/// server draws from, the packages of its mode, and the server's signer, which a server alone
/// holds: a replay or a verifier signs nothing.
#[derive(Debug)]
pub struct SessionSource<'p> {
    pub log: SessionLog,
    pub seeds: ServerSeeds,
    pub packages: &'p ModePackages,
    pub signer: Option<Signer>,
}

/// A checkpoint a session's match resumes at: the segment it starts, and its snapshot.
#[derive(Debug, Clone, Copy)]
pub struct ResumeAt<'s> {
    pub segment: u32,
    pub snapshot: &'s [u8],
}
