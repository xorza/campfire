use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bevy_app::AppExit;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_log::LogEvent;
use campfire_net::{
    Lobby, LobbySetup, RestoredSession, ServerBots, ServerDir, ServerSetup, SessionDir,
    SessionRestored, SimServer,
};
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::Keypair;
use campfire_protocol::{SeedChain, SessionPrivate, SessionTerms};
use campfire_runner::{InputRules, Session};
use tracing::error;

use crate::error::OpeningError;
use crate::server_config::ServerConfig;

/// What a server starts with: a new session, open to its players, or the session a stop ended,
/// to restore. A session past the restore window, or one that ended, has its log published
/// instead, and a new one opens.
#[derive(Debug)]
pub(crate) enum Opening {
    New(Box<Lobby>),
    Restored(Box<Restore>),
}

/// A session to restore in the first frame, as a match starts in a fixed tick.
#[derive(Resource, Debug)]
pub(crate) struct Restore {
    session: RestoredSession,
    packages: ModePackages,
    bots: ServerBots,
}

/// What `Opening::of` needs.
#[derive(Debug)]
pub(crate) struct OpeningSetup<'a> {
    pub(crate) data: &'a ServerDir,
    pub(crate) packages: ModePackages,
    pub(crate) server: ServerSetup,
    pub(crate) bots: ServerBots,
    pub(crate) segments: NonZeroU32,
}

impl Opening {
    /// What the server starts: `found`, the session to restore that `find` gave, or a new one,
    /// its directory, private record and journal made. An error for a session or a journal not
    /// made.
    pub(crate) fn of(
        found: Option<RestoredSession>,
        setup: OpeningSetup<'_>,
    ) -> Result<Opening, OpeningError> {
        if let Some(session) = found {
            return Ok(Opening::Restored(Box::new(Restore {
                session,
                packages: setup.packages,
                bots: setup.bots,
            })));
        }
        Opening::open(setup).map(|lobby| Opening::New(Box::new(lobby)))
    }

    /// The terms of the session it starts.
    pub(crate) fn terms(&self) -> &SessionTerms {
        match self {
            Opening::New(lobby) => lobby.terms(),
            Opening::Restored(restore) => &restore.session.log.header().terms,
        }
    }

    /// The session a stop ended, under the data directory `data`, of the mode `packages` holds,
    /// within the restore window `window`: none when the directory holds none; when its match
    /// never started, whose directory goes; and when it is past the window or ended, whose log
    /// is published, the server key `key` signing its result with auxiliary randomness from
    /// `entropy`. An error for a session that does not read or end.
    pub(crate) fn find(
        data: &ServerDir,
        packages: &ModePackages,
        window: Duration,
        key: Keypair,
        entropy: fn(&mut [u8; 32]),
    ) -> Result<Option<RestoredSession>, OpeningError> {
        let Some(session) = SessionDir::waiting(data).map_err(OpeningError::Waiting)? else {
            return Ok(None);
        };
        let idle = SystemTime::now()
            .duration_since(session.modified)
            .unwrap_or_default();
        if session.log.result().is_none() && idle <= window {
            return Ok(Some(session));
        }
        session
            .abort(data, packages, key, entropy)
            .map_err(OpeningError::Abort)?;
        Ok(None)
    }

    /// A new session of `setup.packages`, its seed chain fresh, its directory, private record
    /// and journal made.
    fn open(setup: OpeningSetup<'_>) -> Result<Lobby, OpeningError> {
        let mut seed = [0; 32];
        (setup.server.entropy)(&mut seed);
        let seed_chain = SeedChain::new(seed, setup.segments);
        let slots = usize::try_from(setup.packages.manifest().slots()).expect("slots fit usize");
        let tick_hz = setup.packages.manifest().tick_hz.default();
        let mut lobby = Lobby::new(LobbySetup {
            packages: Arc::new(setup.packages),
            seed_chain,
            tick_hz,
            inputs: InputRules::LAN,
            slots,
            bots: setup.bots,
            open: Vec::new(),
            server: setup.server,
        })
        .map_err(OpeningError::Lobby)?;
        let private = SessionPrivate {
            seed_chain,
            terms: lobby.terms().clone(),
        };
        let dir = SessionDir::create(setup.data, &private).map_err(OpeningError::NewSession)?;
        let files = dir.start().map_err(OpeningError::NewJournal)?;
        lobby.keep_files(files);
        Ok(lobby)
    }
}

impl Restore {
    /// Restores the session in `world`'s server, its log's bytes from the first frame on; see
    /// `SimServer::restore_match`. An error, logged, ends the app.
    pub(crate) fn run(world: &mut World) {
        let Restore {
            session,
            packages,
            bots,
        } = world
            .remove_resource::<Restore>()
            .expect("a restore runs while one waits");
        let id = session.log.session_id();
        let server = world.resource::<ServerConfig>().0;
        match SimServer::restore_match(world, session, &packages, &server, bots) {
            Ok(()) => {
                let tick = world.resource::<Session>().log().next_tick();
                SessionRestored { session: id, tick }.log();
            }
            Err(error) => {
                error!(%error, "the session's match does not restore");
                world.write_message(AppExit::error());
            }
        }
    }
}
