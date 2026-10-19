use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use campfire_common::ErrorReport;
use campfire_common::ExitStatus;
use campfire_mode::OptionValue;
use campfire_net::{
    ClientDir, LocalPace, LocalRelink, LocalServer, LocalServerSetup, Os, Pace, SeatBotFile,
    ServerPin,
};
use campfire_package::ModePackages;
use campfire_quic::{LinkTimes, QuicEndpoint};
use tracing::error;

use crate::command_line::{Args, Server};

/// The server the client plays on, once it is open, with the client's data directory: a remote
/// one, by its pin and its address, or a local one, running, its data in the directory. Dropped, a
/// local server ends the session and publishes its log.
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
    },
}

/// What a local server starts with: the client's data directory, the mode, the bots, the values
/// the player picks of the mode's options, and the pace it follows.
#[derive(Debug, Clone, Copy)]
struct LocalStart<'a> {
    data: &'a ClientDir,
    packages: &'a Arc<ModePackages>,
    bots: &'a [SeatBotFile],
    options: &'a [OptionValue],
    pace: &'a Arc<Pace>,
}

impl Connection {
    /// The server `args` names, its data directory open; a local one starts, of the mode
    /// `packages` holds, its data in that directory, following `pace`. The exit code when the
    /// directory does not open or a local server does not start.
    pub(crate) fn open(
        args: &Args,
        packages: &Arc<ModePackages>,
        pace: &Arc<Pace>,
    ) -> Result<Self, ExitCode> {
        match &args.server {
            Server::Remote {
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
            Server::Local {
                bots,
                options,
                data,
            } => {
                let data = Self::open_dir(data)?;
                let server = Self::start_local(LocalStart {
                    data: &data,
                    packages,
                    bots,
                    options,
                    pace,
                })?;
                Ok(Self::Local { server, data })
            }
        }
    }

    /// The client's data directory, if it has one.
    pub(crate) fn data(&self) -> Option<Arc<ClientDir>> {
        match self {
            Self::Remote { data, .. } => data.clone(),
            Self::Local { data, .. } => Some(Arc::clone(data)),
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

    /// Links `client`, the client entity of `app`, to the server: over QUIC to a remote one, which
    /// takes only the TLS key the pin names; by in-process channels to a local one, whose `pace`
    /// the app then follows, its session's ticks `tick` long, and whose new link after each load
    /// it takes. The exit code when the client's socket does not open.
    pub(crate) fn link(
        &mut self,
        app: &mut App,
        client: Entity,
        pace: &Arc<Pace>,
        tick: Duration,
    ) -> Result<(), ExitCode> {
        match self {
            Self::Remote { pin, address, .. } => {
                let endpoint = QuicEndpoint::client(*address, &pin.tls_key, LinkTimes::CAMPFIRE).map_err(
                    |error| {
                        error!(error = %ErrorReport::of(&error), "the client's socket does not open");
                        ExitCode::from(ExitStatus::Failure)
                    },
                )?;
                app.world_mut().entity_mut(client).insert(endpoint);
            }
            Self::Local { server, .. } => {
                app.world_mut()
                    .entity_mut(client)
                    .insert(server.take_link());
                app.add_plugins((
                    LocalPace {
                        pace: Arc::clone(pace),
                        tick,
                    },
                    LocalRelink {
                        relinks: server.relinks(),
                    },
                ));
            }
        }
        Ok(())
    }

    /// Starts the local server of `start`'s mode, its data in the client's data directory; the
    /// exit code when it does not start.
    fn start_local(start: LocalStart<'_>) -> Result<LocalServer, ExitCode> {
        let LocalStart {
            data,
            packages,
            bots,
            options,
            pace,
        } = start;
        let slots = bots
            .iter()
            .map(SeatBotFile::read)
            .collect::<Result<_, _>>()
            .map_err(|error| {
                error!(error = %ErrorReport::of(&error), "a server bot's orders file does not read");
                ExitCode::from(ExitStatus::Failure)
            })?;
        let data = data.layout().local_server_dir();
        LocalServer::start(LocalServerSetup {
            packages: Arc::clone(packages),
            data,
            bots: slots,
            options: options.to_vec(),
            pace: Arc::clone(pace),
            os: Os::SYSTEM,
        })
        .map_err(|error| {
            error!(error = %ErrorReport::of(&error), "the local server does not start");
            ExitCode::from(ExitStatus::Failure)
        })
    }
}
