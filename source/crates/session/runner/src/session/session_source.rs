use campfire_package::ModePackages;
use campfire_protocol::{ServerSeeds, SessionLog};

/// What a session's match is built from: its log, rewound to replay its ticks, the seeds its
/// server draws from, and the packages of its mode.
#[derive(Debug)]
pub struct SessionSource<'p> {
    pub log: SessionLog,
    pub seeds: ServerSeeds,
    pub packages: &'p ModePackages,
}

/// A checkpoint a session's match resumes at: the segment it starts, and its snapshot.
#[derive(Debug, Clone, Copy)]
pub struct ResumeAt<'s> {
    pub segment: u32,
    pub snapshot: &'s [u8],
}
