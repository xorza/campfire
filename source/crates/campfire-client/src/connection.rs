use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use campfire_net::{
    ClientDir, LocalPace, LocalRelink, LocalServer, LocalServerSetup, Pace, ServerPin, SlotBot,
};
use campfire_package::ModePackages;
use lightyear::prelude::client::WebTransportClientIo;
use lightyear::prelude::{LocalAddr, PeerAddr};
use tracing::error;

use crate::{Args, BotFile, Server};

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
                certificate,
                key,
                tick_hz,
            } => Ok(Connection::Remote {
                pin: ServerPin {
                    key,
                    certificate,
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

    /// Links `client`, the client entity of `app`, to the server: over WebTransport to a remote
    /// one; by in-process channels to a local one, whose `pace` the app then follows, its
    /// session's ticks `tick` long, and whose new link after each load it takes.
    pub(crate) fn link(&mut self, app: &mut App, client: Entity, pace: &Arc<Pace>, tick: Duration) {
        match self {
            Connection::Remote { pin, address } => {
                app.world_mut().entity_mut(client).insert((
                    WebTransportClientIo {
                        certificate_digest: pin.certificate.to_string(),
                        target: None,
                    },
                    PeerAddr(*address),
                    // A raw client counts as connected once linked only with a local address,
                    // which only names it; the transport binds its own socket.
                    LocalAddr(SocketAddr::from(([0, 0, 0, 0], 0))),
                ));
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
    }

    /// Starts the local server of the mode `packages` holds, its data in the client's data
    /// directory `data`, its bots those of `bots`, following `pace`; the exit code when it does
    /// not start.
    fn start_local(
        data: Option<&ClientDir>,
        packages: &Arc<ModePackages>,
        bots: &[BotFile],
        pace: &Arc<Pace>,
    ) -> Result<LocalServer, ExitCode> {
        let mut slots = Vec::with_capacity(bots.len());
        for bot in bots {
            let script = crate::read_script(&bot.path).map_err(|problem| {
                error!(%problem, "a server bot's orders file does not read");
                ExitCode::FAILURE
            })?;
            slots.push(SlotBot::new(bot.slot, script));
        }
        let data = data.expect("--local has --data").local_server_dir();
        LocalServer::start(LocalServerSetup {
            packages: Arc::clone(packages),
            data,
            bots: slots,
            pace: Arc::clone(pace),
            clock: crate::unix_now,
            entropy: crate::fill,
        })
        .map_err(|error| {
            error!(%error, "the local server does not start");
            ExitCode::FAILURE
        })
    }
}
