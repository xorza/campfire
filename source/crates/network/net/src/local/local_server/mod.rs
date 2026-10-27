use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bevy_app::AppExit;
use campfire_common::{PlayerSlot, Tick};
use campfire_mode::{OptionValue, PlayerSlotPick};
use campfire_package::ModePackages;
use campfire_protocol::SessionLog;
use campfire_runner::SessionRules;
use campfire_threads::{Job, Worker};
use campfire_visibility::{HostVisibility, SettingValue, SpectatorFog};
use tracing::error;

use crate::local::local_link::LocalLink;
use crate::local::local_server::error::LocalServerError;
use crate::sim_server::lobby::{Lobby, LobbyOpening, LobbyStart};
use crate::sim_server::pending_replay::PendingReplay;
use crate::sim_server::server_app_setup::{ServerAppSetup, ServerRun};
use crate::sim_server::sim_server_world::SessionOpening;
use crate::udp_links::{LocalPair, UdpLinks};
use campfire_host::JoinRole;
use campfire_host::KeyFile;
use campfire_host::LogSeen;
use campfire_host::Os;
use campfire_host::Pace;
use campfire_host::ServerDir;
use campfire_host::ServerPin;
use campfire_host::ServerSetup;
use campfire_host::SessionTimes;
use campfire_host::{ClientLayout, PlannedBot, ServerBots};
use campfire_host::{Publisher, SessionDir};

pub(crate) mod error;

/// A session's server on a thread of a singleplayer client's process, linked to the client by a
/// sealed link over the emulator: the client's player in player slot 0, in each other player slot
/// the server's bot its setup names, or none, open for a late join; or the replay of a report,
/// which its client watches. Its data directory holds what a
/// dedicated server's does; a session an earlier start left ends aborted, its log published, and
/// each start opens a new one. It runs until it drops: it then ends the session, as the mode ended
/// the match or aborted, publishes its log, and its thread ends. Both ends follow the setup's pace.
pub struct LocalServer {
    stop: Arc<AtomicBool>,
    pin: ServerPin,
    /// The client's end of the link, until the client takes it.
    link: Option<LocalLink>,
    /// Dropped after the server's `Drop` asks it to stop.
    _worker: Worker,
}

/// What a local server starts with.
#[derive(Debug)]
pub struct LocalServerSetup {
    pub packages: Arc<ModePackages>,
    /// Its data directory, which it holds locked while it runs.
    pub data: PathBuf,
    pub session: LocalSession,
    pub pace: Arc<Pace>,
    pub os: Os,
    /// Its client's data directory, whose reports it writes its log into.
    pub reports: ClientLayout,
    /// What the tap of its client's console saw, with a console: it then answers the requests
    /// the client forwards, and holds the match paused after its first tick.
    pub console: Option<Arc<Mutex<LogSeen>>>,
}

/// The session a local server opens: a new one, or the replay of a player's report.
#[derive(Debug)]
pub enum LocalSession {
    New(LocalLobby),
    Replay(Box<LocalReplay>),
}

/// A new session's lobby on a local server.
#[derive(Debug)]
pub struct LocalLobby {
    /// The bots it plays; one in player slot 0 while its client plays leaves no player slot to it,
    /// which the lobby refuses.
    pub bots: Vec<PlannedBot>,
    /// Whether its client plays player slot 0, or watches with no player slot (08-sessions.md, A
    /// local watcher takes no player slot and sees every unit).
    pub role: JoinRole,
    /// The values the player picks of the mode's options, the others their defaults.
    pub options: Vec<OptionValue>,
    /// The values the player picks of each player slot's choices of values, the others none.
    pub player_slot_picks: Vec<PlayerSlotPick>,
}

/// A player's report on a local server (08-sessions.md, The client opens a report): its log,
/// which this server's key signed, and the tick the replay runs through and holds paused after,
/// the last its client ran.
#[derive(Debug)]
pub struct LocalReplay {
    pub log: SessionLog,
    pub through: Tick,
}

impl LocalServer {
    /// Opens the local server's data directory and session, and starts its thread. An error when
    /// the directory does not open, a session an earlier start left does not end, or the new
    /// one does not open.
    pub fn start(setup: LocalServerSetup) -> Result<Self, LocalServerError> {
        let LocalServerSetup {
            packages,
            data,
            session,
            pace,
            os,
            reports,
            console,
        } = setup;
        let dir = ServerDir::open(&data).map_err(LocalServerError::Data)?;
        let signer = KeyFile::read_or_create(&dir.layout().key_file(), os.entropy)
            .map_err(LocalServerError::Key)?;
        let signer = Arc::new(signer);
        let key = signer.public_key();
        let tls_key = UdpLinks::server(&signer)
            .tls_key()
            .expect("a raw key's identity");
        Self::end_earlier(Publisher {
            data: &dir,
            packages: &packages,
            signer: &signer,
        })?;
        let server = ServerSetup {
            signer,
            times: SessionTimes::DEFAULT,
            os,
        };
        let opening = match session {
            LocalSession::New(lobby) => {
                SessionOpening::New(Box::new(Self::lobby(lobby, &packages, &server, &dir)?))
            }
            LocalSession::Replay(replay) => {
                SessionOpening::Replayed(Box::new(Self::replay(replay, &packages, &server, &pace)?))
            }
        };
        let tick_hz = opening.terms().tick_hz;
        let LocalPair {
            server: link,
            client,
        } = UdpLinks::local(&server.signer, server.os.entropy);
        let stop = Arc::new(AtomicBool::new(false));
        let session = opening.span();
        let setup = ServerAppSetup {
            opening,
            data: dir,
            endpoint: link,
            run: ServerRun::Local {
                stop: Arc::clone(&stop),
                pace,
                reports,
                console,
            },
        };
        let worker = Worker::start(Job::LocalServer, move || {
            let _session = session.entered();
            if let AppExit::Error(code) = setup.build().run() {
                error!(code = code.get(), "the local server stopped with an error");
            }
        });
        Ok(Self {
            stop,
            pin: ServerPin {
                key,
                tls_key,
                tick_hz,
            },
            link: Some(LocalLink::new(client)),
            _worker: worker,
        })
    }

    /// The lobby of a new session of `packages` on `server`, its files in `dir`: the client's
    /// player in player slot 0, or watching, the bots in their player slots, every other open; an
    /// error when a bot or the session does not open.
    fn lobby(
        lobby: LocalLobby,
        packages: &Arc<ModePackages>,
        server: &ServerSetup,
        dir: &ServerDir,
    ) -> Result<Lobby, LocalServerError> {
        let LocalLobby {
            bots,
            role,
            options,
            player_slot_picks,
        } = lobby;
        let bots = ServerBots::new(bots, None, packages).map_err(LocalServerError::Bots)?;
        let player_slots =
            u32::try_from(packages.manifest().player_slots()).expect("player slots fit u32");
        // A client that watches holds no player slot, so player slot 0 is a bot's or open; and the
        // local server admits it as its one spectator, who sees every unit.
        let (first_open, host, start) = match role {
            JoinRole::Play => (1, HostVisibility::default(), LobbyStart::Players),
            JoinRole::Watch { .. } => {
                let host = HostVisibility {
                    settings: vec![SettingValue::Spectators(SpectatorFog::All)],
                    max_spectators: Some(1),
                };
                (0, host, LobbyStart::Watched)
            }
        };
        let open = (first_open..player_slots)
            .map(PlayerSlot::new)
            .filter(|&player_slot| {
                bots.player_slots
                    .iter()
                    .all(|bot| bot.player_slot != player_slot)
            })
            .collect();
        let opening = LobbyOpening {
            packages: Arc::clone(packages),
            bots,
            open,
            host,
            options,
            player_slot_picks,
            server: server.clone(),
            start,
        };
        Lobby::open(opening, dir).map_err(LocalServerError::Open)
    }

    /// The replay of `replay`'s log of `packages` on `server`, which holds `pace` paused from the
    /// start (08-sessions.md, The client opens a report); an error when this server's key did not
    /// sign the log, the log did not run the tick, reveals no seed, or its terms are not the
    /// mode's.
    fn replay(
        replay: Box<LocalReplay>,
        packages: &Arc<ModePackages>,
        server: &ServerSetup,
        pace: &Arc<Pace>,
    ) -> Result<PendingReplay, LocalServerError> {
        let LocalReplay { log, through } = *replay;
        let terms = &log.header().terms;
        if terms.server_key != server.signer.public_key() {
            return Err(LocalServerError::OtherServer);
        }
        let end = log.next_tick();
        if through >= end {
            return Err(LocalServerError::PastLog { through, end });
        }
        let seeds = log.revealed_seeds().ok_or(LocalServerError::Unrevealed)?;
        let checked = SessionRules::of(packages)
            .check(terms)
            .map_err(LocalServerError::Terms)?;
        pace.set_paused(true);
        Ok(PendingReplay {
            log,
            seeds,
            through,
            packages: Arc::clone(packages),
            server: server.clone(),
            checked,
            pace: Arc::clone(pace),
        })
    }

    /// What the client pins: the server's key, its TLS key and its tick rate.
    pub const fn pin(&self) -> ServerPin {
        self.pin
    }

    /// The client's end of the link, once.
    pub const fn take_link(&mut self) -> LocalLink {
        self.link.take().expect("the client takes the link once")
    }

    /// Ends aborted a session an earlier start left under the publisher's data directory, and
    /// publishes its log by `publisher`; removes one whose match never started.
    fn end_earlier(publisher: Publisher<'_>) -> Result<(), LocalServerError> {
        if let Some(session) =
            SessionDir::waiting(publisher.data).map_err(LocalServerError::Waiting)?
        {
            session.abort(publisher).map_err(LocalServerError::Abort)?;
        }
        Ok(())
    }
}

impl fmt::Debug for LocalServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalServer")
            .field("stop", &self.stop)
            .field("pin", &self.pin)
            .field("link_taken", &self.link.is_none())
            .finish_non_exhaustive()
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests;
