use std::net::SocketAddr;
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

/// The server the client plays on, once it is open: a remote one, by its pin and its address, or
/// a local one, running. Dropped, a local server ends the session and publishes its log.
#[derive(Debug)]
pub(crate) enum Connection {
    Remote { pin: ServerPin, address: SocketAddr },
    Local(LocalServer),
}

impl Connection {
    /// The server `args` names; a local one starts, of the mode `packages` holds, its data in
    /// the client's data directory `data`, following `pace`. The exit code when a local server
    /// does not start.
    pub(crate) fn open(
        args: &Args,
        data: Option<&ClientDir>,
        packages: &Arc<ModePackages>,
        pace: &Arc<Pace>,
    ) -> Result<Connection, ExitCode> {
        match &args.server {
            &Server::Remote {
                address,
                tls_key,
                key,
                tick_hz,
            } => Ok(Connection::Remote {
                pin: ServerPin {
                    key,
                    tls_key,
                    tick_hz,
                },
                address,
            }),
            Server::Local { bots } => {
                Connection::start_local(data, packages, bots, pace).map(Connection::Local)
            }
        }
    }

    /// What the client pins of the server.
    pub(crate) const fn pin(&self) -> ServerPin {
        match self {
            Connection::Remote { pin, .. } => *pin,
            Connection::Local(local) => local.pin(),
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
            Connection::Remote { pin, address } => {
                let endpoint = QuicEndpoint::client(*address, &pin.tls_key, LinkTimes::CAMPFIRE).map_err(
                    |error| {
                        error!(error = %ErrorReport::of(&error), "the client's socket does not open");
                        ExitCode::from(ExitStatus::Failure)
                    },
                )?;
                app.world_mut().entity_mut(client).insert(endpoint);
            }
            Connection::Local(local) => {
                app.world_mut().entity_mut(client).insert(local.take_link());
                app.add_plugins((
                    LocalPace {
                        pace: Arc::clone(pace),
                        tick,
                    },
                    LocalRelink {
                        relinks: local.relinks(),
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
        data: Option<&ClientDir>,
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
        let data = data
            .expect("--local has --data")
            .layout()
            .local_server_dir();
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
