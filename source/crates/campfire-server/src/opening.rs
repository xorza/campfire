use std::num::NonZeroU32;
use std::path::Path;
use std::time::{Duration, SystemTime};

use bevy_app::AppExit;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_log::LogEvent;
use campfire_net::{
    Lobby, LobbySetup, RestoredSession, SessionAborted, SessionDir, SessionRestored, SimServer,
};
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::Keypair;
use campfire_protocol::{CertificateHash, SeedChain, SessionPrivate, SessionTerms};
use campfire_runner::{InputRules, Session};
use tracing::error;

use crate::error::OpeningError;
use crate::server_key::ServerKey;

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
}

/// What `Opening::of` needs.
#[derive(Debug)]
pub(crate) struct OpeningSetup<'a> {
    pub(crate) data: &'a Path,
    pub(crate) packages: ModePackages,
    pub(crate) key: &'a Keypair,
    pub(crate) certificate: CertificateHash,
    /// How long after its journal's last write a session restores.
    pub(crate) window: Duration,
    pub(crate) segments: NonZeroU32,
    pub(crate) entropy: fn(&mut [u8; 32]),
    pub(crate) clock: fn() -> u64,
}

impl Opening {
    /// What the data directory `setup.data` holds to start: the session to restore, or a new
    /// one, its directory, private record and journal made. An error for a data directory whose
    /// session does not read or end, and for a session or a journal not made.
    pub(crate) fn of(setup: OpeningSetup<'_>) -> Result<Opening, OpeningError> {
        if let Some(session) = Opening::find(&setup)? {
            return Ok(Opening::Restored(Box::new(Restore {
                session,
                packages: setup.packages,
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

    /// The session a stop ended, within the window: none when the data directory holds none;
    /// when its match never started, whose directory goes; and when it is past the window or
    /// ended, whose log is published.
    fn find(setup: &OpeningSetup<'_>) -> Result<Option<RestoredSession>, OpeningError> {
        let data = setup.data;
        let Some(dir) = SessionDir::find(data).map_err(OpeningError::Find)? else {
            return Ok(None);
        };
        let Some(session) = dir.restore().map_err(OpeningError::Restore)? else {
            dir.remove().map_err(OpeningError::Remove)?;
            return Ok(None);
        };
        let idle = SystemTime::now()
            .duration_since(session.modified)
            .unwrap_or_default();
        if session.log.result().is_none() && idle <= setup.window {
            return Ok(Some(session));
        }
        let id = session.log.session_id();
        let mut aux = [0; 32];
        (setup.entropy)(&mut aux);
        let file = session
            .abort(data, &setup.packages, setup.key, &aux)
            .map_err(OpeningError::Abort)?;
        SessionAborted { session: id, file }.log();
        Ok(None)
    }

    /// A new session of `setup.packages`, its seed chain fresh, its directory, private record
    /// and journal made.
    fn open(setup: OpeningSetup<'_>) -> Result<Lobby, OpeningError> {
        let mut seed = [0; 32];
        (setup.entropy)(&mut seed);
        let seed_chain = SeedChain::new(seed, setup.segments);
        let players = usize::try_from(setup.packages.manifest().slots()).expect("slots fit usize");
        let tick_hz = setup.packages.manifest().tick_hz.default();
        let mut lobby = Lobby::new(LobbySetup {
            packages: setup.packages,
            server_key: setup.key.x_only_public_key().0,
            seed_chain,
            tick_hz,
            inputs: InputRules::LAN,
            certificate: setup.certificate,
            players,
            clock: setup.clock,
            entropy: setup.entropy,
        })
        .expect("a mode runs at its default rate");
        let private = SessionPrivate {
            seed_chain,
            terms: lobby.terms().clone(),
        };
        let dir = SessionDir::create(setup.data, &private).map_err(OpeningError::NewSession)?;
        let journal = dir.start_journal().map_err(OpeningError::NewJournal)?;
        lobby.keep_journal(journal);
        Ok(lobby)
    }
}

impl Restore {
    /// Restores the session in `world`'s server, its log's bytes from the first frame on; see
    /// `SimServer::restore_match`. An error, logged, ends the app.
    pub(crate) fn run(world: &mut World) {
        let Restore { session, packages } = world
            .remove_resource::<Restore>()
            .expect("a restore runs while one waits");
        let id = session.log.session_id();
        let key = world.resource::<ServerKey>().0;
        match SimServer::restore_match(world, session, &packages, &key, crate::fill) {
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
