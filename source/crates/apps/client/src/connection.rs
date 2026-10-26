use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use campfire_common::{ErrorReport, ExitStatus};
use campfire_host::{ClientDir, Hosting, JoinRole, Os, Pace, PlayerSlotBotArg, ServerPin};
use campfire_net::{
    LocalLobby, LocalPacePlugin, LocalReplay, LocalServer, LocalServerSetup, LocalSession, UdpLinks,
};
use campfire_net_endpoint::Endpoint;
use campfire_net_socket::ClientSocket;
use campfire_package::ModePackages;
use campfire_sim::MatchRate;
use tracing::error;

use crate::command_line::{Args, PlayedServer};

/// The server the client plays on, once it is open, with the client's data directory: a remote
/// one, by its pin and its address, or a local one, running, its data in the directory, with the
/// pace both ends follow. Dropped, a local server ends the session and publishes its log.
#[derive(Debug)]
pub(crate) enum Connection {
    Remote {
        pin: ServerPin,
        address: SocketAddr,
        data: Option<Arc<ClientDir>>,
    },
    Local {
        server: LocalServer,
        data: Arc<ClientDir>,
        pace: Arc<Pace>,
    },
}

impl Connection {
    /// The server `args` names, its data directory open; a local one starts, of the mode
    /// `packages` holds, for a client of `role`, its data in that directory, at a pace of its
    /// own. The exit code when the directory does not open or a local server does not start.
    pub(crate) fn open(
        args: &Args,
        role: JoinRole,
        packages: &Arc<ModePackages>,
    ) -> Result<Self, ExitCode> {
        match &args.server {
            PlayedServer::Remote {
                address,
                tls_key,
                key,
                tick_hz,
                data,
            } => Ok(Self::Remote {
                pin: ServerPin {
                    key: *key,
                    tls_key: *tls_key,
                    tick_hz: *tick_hz,
                },
                address: *address,
                data: data.as_deref().map(Self::open_dir).transpose()?,
            }),
            PlayedServer::Local {
                bots,
                options,
                player_slot_picks,
                data,
            } => {
                let data = Self::open_dir(data)?;
                let bots = bots
                    .iter()
                    .map(PlayerSlotBotArg::read)
                    .collect::<Result<_, _>>()
                    .map_err(|error| {
                        error!(error = %ErrorReport::of(&error), "a server bot's orders file does not read");
                        ExitCode::from(ExitStatus::Failure)
                    })?;
                let session = LocalSession::New(LocalLobby {
                    bots,
                    role,
                    options: options.clone(),
                    player_slot_picks: player_slot_picks.clone(),
                });
                Self::open_local(&data, packages, session)
            }
            PlayedServer::Replay {
                report,
                through,
                data,
                ..
            } => {
                let data = Self::open_dir(data)?;
                let log = report.read_log().map_err(|error| {
                    error!(report = %report.path().display(), error = %ErrorReport::of(&error), "the report's log does not read");
                    ExitCode::from(ExitStatus::Failure)
                })?;
                let session = LocalSession::Replay(Box::new(LocalReplay {
                    log,
                    through: *through,
                }));
                Self::open_local(&data, packages, session)
            }
        }
    }

    /// The local server of the mode `packages` holds, opening `session`, its data in the
    /// client's data directory `data`, at a pace of its own; the exit code when it does not start.
    fn open_local(
        data: &Arc<ClientDir>,
        packages: &Arc<ModePackages>,
        session: LocalSession,
    ) -> Result<Self, ExitCode> {
        let pace = Arc::new(Pace::default());
        let reports = data.layout().clone();
        let server = LocalServer::start(LocalServerSetup {
            packages: Arc::clone(packages),
            data: reports.local_server_dir(),
            session,
            pace: Arc::clone(&pace),
            os: Os::SYSTEM,
            reports,
        })
        .map_err(|error| {
            error!(error = %ErrorReport::of(&error), "the local server does not start");
            ExitCode::from(ExitStatus::Failure)
        })?;
        Ok(Self::Local {
            server,
            data: Arc::clone(data),
            pace,
        })
    }

    /// The client's data directory, if it has one.
    pub(crate) fn data(&self) -> Option<Arc<ClientDir>> {
        match self {
            Self::Remote { data, .. } => data.clone(),
            Self::Local { data, .. } => Some(Arc::clone(data)),
        }
    }

    /// Whether the server runs on a thread of the client's process.
    pub(crate) const fn hosting(&self) -> Hosting {
        match self {
            Self::Remote { .. } => Hosting::Remote,
            Self::Local { .. } => Hosting::Local,
        }
    }

    /// The pace of a local match, which the player's keys set; none for a remote one.
    pub(crate) const fn pace(&self) -> Option<&Arc<Pace>> {
        match self {
            Self::Remote { .. } => None,
            Self::Local { pace, .. } => Some(pace),
        }
    }

    /// The client's data directory at `path`; the exit code when it does not open.
    fn open_dir(path: &Path) -> Result<Arc<ClientDir>, ExitCode> {
        ClientDir::open(path).map(Arc::new).map_err(|error| {
            error!(error = %ErrorReport::of(&error), "the data directory does not open");
            ExitCode::from(ExitStatus::Failure)
        })
    }

    /// What the client pins of the server.
    pub(crate) const fn pin(&self) -> ServerPin {
        match self {
            Self::Remote { pin, .. } => *pin,
            Self::Local { server, .. } => server.pin(),
        }
    }

    /// Links `client`, the client entity of `app`, to the server: over the stack's sealed UDP to a
    /// remote one, which takes only the TLS key the pin names; over the sealed emulated link to a
    /// local one, whose pace the app then follows, at the session's rate, and whose new link after
    /// each load it takes. The exit code when the client's socket does not open.
    pub(crate) fn link(&mut self, app: &mut App, client: Entity) -> Result<(), ExitCode> {
        match self {
            Self::Remote { pin, address, .. } => {
                let socket = ClientSocket::bind(*address, UdpLinks::client(pin.tls_key)).map_err(
                    |error| {
                        error!(error = %ErrorReport::of(&error), "the client's socket does not open");
                        ExitCode::from(ExitStatus::Failure)
                    },
                )?;
                app.world_mut()
                    .entity_mut(client)
                    .insert(Endpoint::from(socket.endpoint()));
            }
            Self::Local { server, pace, .. } => {
                app.world_mut()
                    .entity_mut(client)
                    .insert(server.take_link());
                app.add_plugins(LocalPacePlugin {
                    pace: Arc::clone(pace),
                    tick_rate: MatchRate::new(server.pin().tick_hz).rate(),
                });
            }
        }
        Ok(())
    }
}
