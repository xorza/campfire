use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy_app::AppExit;
use campfire_common::PlayerSlot;
use campfire_mode::{OptionValue, PlayerSlotPick};
use campfire_package::ModePackages;
use campfire_threads::{Job, Worker};
use campfire_visibility::{HostVisibility, SettingValue, SpectatorFog};
use tracing::error;

use crate::local::local_link::LocalLink;
use crate::local::local_server::error::LocalServerError;
use crate::sim_server::lobby::{Lobby, LobbyOpening, LobbyStart};
use crate::sim_server::server_app_setup::{ServerAppSetup, ServerRun};
use crate::sim_server::sim_server_world::SessionOpening;
use crate::udp_links::{LocalPair, UdpLinks};
use campfire_host::JoinRole;
use campfire_host::KeyFile;
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
/// the server's bot its setup names, or none, open for a late join. Its data directory holds what a
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
    pub pace: Arc<Pace>,
    pub os: Os,
    /// Its client's data directory, whose reports it writes its log into.
    pub reports: ClientLayout,
}

impl LocalServer {
    /// Opens the local server's data directory and session, and starts its thread. An error when
    /// the directory does not open, a session an earlier start left does not end, or the new
    /// one does not open.
    pub fn start(setup: LocalServerSetup) -> Result<Self, LocalServerError> {
        let LocalServerSetup {
            packages,
            data,
            bots,
            options,
            player_slot_picks,
            role,
            pace,
            os,
            reports,
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
        let bots = ServerBots::new(bots, None, &packages).map_err(LocalServerError::Bots)?;
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
            packages: Arc::clone(&packages),
            bots,
            open,
            host,
            options,
            player_slot_picks,
            server: server.clone(),
            start,
        };
        let lobby = Lobby::open(opening, &dir).map_err(LocalServerError::Open)?;
        let tick_hz = lobby.terms().tick_hz;
        let LocalPair {
            server: link,
            client,
        } = UdpLinks::local(&server.signer, server.os.entropy);
        let stop = Arc::new(AtomicBool::new(false));
        let opening = SessionOpening::New(Box::new(lobby));
        let session = opening.span();
        let setup = ServerAppSetup {
            opening,
            data: dir,
            endpoint: link,
            run: ServerRun::Local {
                stop: Arc::clone(&stop),
                pace,
                reports,
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
