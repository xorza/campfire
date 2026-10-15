use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use campfire_common::ExitStatus;
use campfire_log::ErrorReport;
use campfire_net::{
    ClientDir, LocalPace, LocalRelink, LocalServer, LocalServerSetup, Os, Pace, ServerPin,
    SlotBotFile,
};
use campfire_package::ModePackages;
use campfire_quic::{LinkTimes, QuicEndpoint};
use tracing::error;

use crate::args::{Args, Server};

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

impl Connection {
    /// The server `args` names, its data directory open; a local one starts, of the mode
    /// `packages` holds, its data in that directory, following `pace`. The exit code when the
    /// directory does not open or a local server does not start.
    pub(crate) fn open(
        args: &Args,
        packages: &Arc<ModePackages>,
        pace: &Arc<Pace>,
    ) -> Result<Connection, ExitCode> {
        match &args.server {
            Server::Remote {
                address,
                tls_key,
                key,
                tick_hz,
                data,
            } => Ok(Connection::Remote {
                pin: ServerPin {
                    key: *key,
                    tls_key: *tls_key,
                    tick_hz: *tick_hz,
                },
                address: *address,
                data: data.as_deref().map(Connection::open_dir).transpose()?,
            }),
            Server::Local { bots, data } => {
                let data = Connection::open_dir(data)?;
                let server = Connection::start_local(&data, packages, bots, pace)?;
                Ok(Connection::Local { server, data })
            }
        }
    }

    /// The client's data directory, if it has one.
    pub(crate) fn data(&self) -> Option<Arc<ClientDir>> {
        match self {
            Connection::Remote { data, .. } => data.clone(),
            Connection::Local { data, .. } => Some(Arc::clone(data)),
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
            Connection::Remote { pin, .. } => *pin,
            Connection::Local { server, .. } => server.pin(),
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
            Connection::Remote { pin, address, .. } => {
                let endpoint = QuicEndpoint::client(*address, &pin.tls_key, LinkTimes::CAMPFIRE).map_err(
                    |error| {
                        error!(error = %ErrorReport::of(&error), "the client's socket does not open");
                        ExitCode::from(ExitStatus::Failure)
                    },
                )?;
                app.world_mut().entity_mut(client).insert(endpoint);
            }
            Connection::Local { server, .. } => {
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

    /// Starts the local server of the mode `packages` holds, its data in the client's data
    /// directory `data`, its bots those of `bots`, following `pace`; the exit code when it does
    /// not start.
    fn start_local(
        data: &ClientDir,
        packages: &Arc<ModePackages>,
        bots: &[SlotBotFile],
        pace: &Arc<Pace>,
    ) -> Result<LocalServer, ExitCode> {
        let slots = bots
            .iter()
            .map(SlotBotFile::read)
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
            pace: Arc::clone(pace),
            os: Os::SYSTEM,
        })
        .map_err(|error| {
            error!(error = %ErrorReport::of(&error), "the local server does not start");
            ExitCode::from(ExitStatus::Failure)
        })
    }
}
