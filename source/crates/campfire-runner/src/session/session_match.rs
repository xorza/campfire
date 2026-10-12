use bevy_ecs::world::World;
use campfire_capabilities::Match;
use campfire_package::ModePackages;
use campfire_protocol::{ServerSeeds, SessionLog};

use crate::session::error::{ResumeError, StartError};
use crate::session::session_world::SessionWorld;
use crate::session::{Session, SessionParts};

/// A session's match in a world of its own, as a verifier replays a log in: no network layer,
/// and nothing but the session drives its ticks.
pub trait SessionMatch: Sized {
    /// The match of `log`, started as `SessionWorld::start_session` starts it.
    fn start_session(
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
    ) -> Result<Self, StartError>;

    /// The match of `log`, resumed from the checkpoint that starts `segment`, as
    /// `SessionWorld::resume_session` resumes it.
    fn resume_session(
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
        segment: u32,
        snapshot: &[u8],
    ) -> Result<Self, ResumeError>;

    /// Loads the save that starts segment `segment`, whose snapshot is `snapshot`: the log goes
    /// back to it, as `SessionLog::load` says, and the match of the mode `packages` holds
    /// resumes from it, as `resume_session` does.
    fn load_save(
        self,
        segment: u32,
        snapshot: &[u8],
        packages: &ModePackages,
    ) -> Result<Self, ResumeError>;
}

impl SessionMatch for Match {
    fn start_session(
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
    ) -> Result<Match, StartError> {
        let mut world = World::new();
        world.start_session(log, seeds, packages)?;
        Ok(Match::from_built(world))
    }

    fn resume_session(
        log: SessionLog,
        seeds: ServerSeeds,
        packages: &ModePackages,
        segment: u32,
        snapshot: &[u8],
    ) -> Result<Match, ResumeError> {
        let mut world = World::new();
        world.resume_session(log, seeds, packages, segment, snapshot)?;
        Ok(Match::from_built(world))
    }

    fn load_save(
        mut self,
        segment: u32,
        snapshot: &[u8],
        packages: &ModePackages,
    ) -> Result<Match, ResumeError> {
        let session = self
            .remove_resource::<Session>()
            .expect("a session's match holds its session");
        let SessionParts { mut log, seeds } = session.into_parts();
        log.load(segment).map_err(ResumeError::Load)?;
        Match::resume_session(log.rewound(), seeds, packages, segment, snapshot)
    }
}
