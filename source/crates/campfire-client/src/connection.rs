use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use campfire_net::{LocalPace, LocalServer, LocalServerSetup, Pace, ServerPin, SlotBot};
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
    /// The server `args` names; a local one starts, following `pace`. The exit code when a local
    /// server does not start.
    pub(crate) fn open(args: &Args, pace: &Arc<Pace>) -> Result<Connection, ExitCode> {
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
                Connection::start_local(args, bots, pace).map(Connection::Local)
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
    /// session's ticks `tick` long.
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
                app.add_plugins(LocalPace {
                    pace: Arc::clone(pace),
                    tick,
                });
            }
        }
    }

    /// Starts the local server of the mode `args` names, its data in `server` under the
    /// client's data directory, its bots those of `bots`, following `pace`; the exit code when
    /// it does not start.
    fn start_local(
        args: &Args,
        bots: &[BotFile],
        pace: &Arc<Pace>,
    ) -> Result<LocalServer, ExitCode> {
        let packages = crate::load_mode(args)?;
        let mut slots = Vec::with_capacity(bots.len());
        for bot in bots {
            let script = crate::read_script(&bot.path).map_err(|problem| {
                error!(%problem, "a server bot's orders file does not read");
                ExitCode::FAILURE
            })?;
            slots.push(SlotBot::new(bot.slot, script));
        }
        let data = args
            .data
            .as_ref()
            .expect("--local has --data")
            .join("server");
        LocalServer::start(LocalServerSetup {
            packages,
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
