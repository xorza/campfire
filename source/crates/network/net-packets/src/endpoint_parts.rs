use core::net::SocketAddr;

use campfire_net_emulator::EmulatedIo;

use crate::emulated_wires::EmulatedWires;
use crate::endpoint_config::EndpointConfig;
use crate::establish::{ClientEstablish, ServerEstablish};
use crate::plain_establishment::PlainEstablishment;

/// What an endpoint is made of: its IO, the establishment that makes its sessions, its side and
/// its config. An IO crate gives the parts, and a host builds its endpoint from them, so no IO
/// crate depends on the host (13-net-stack/session.md, An IO crate gives the parts of an endpoint).
#[derive(Debug)]
pub struct EndpointParts<Io> {
    pub(crate) io: Io,
    pub(crate) side: EndpointSide,
    pub(crate) config: EndpointConfig,
}

/// Whether an endpoint opens one session to its server or takes sessions, with the
/// establishment of that side.
#[derive(Debug)]
pub(crate) enum EndpointSide {
    Client {
        server: SocketAddr,
        establish: Box<dyn ClientEstablish>,
    },
    Server {
        establish: Box<dyn ServerEstablish>,
    },
}

impl<Io> EndpointParts<Io> {
    /// A client's endpoint over `io`, toward `server`, whose sessions `establish` makes.
    pub fn client(
        io: Io,
        server: SocketAddr,
        establish: Box<dyn ClientEstablish>,
        config: EndpointConfig,
    ) -> Self {
        Self {
            io,
            side: EndpointSide::Client { server, establish },
            config,
        }
    }

    /// A server's endpoint over `io`, whose sessions `establish` makes.
    pub fn server(io: Io, establish: Box<dyn ServerEstablish>, config: EndpointConfig) -> Self {
        Self {
            io,
            side: EndpointSide::Server { establish },
            config,
        }
    }
}

impl EndpointParts<EmulatedWires> {
    /// A client's endpoint of plain sessions over the simulated network's `wire`.
    pub fn plain_client(wire: EmulatedIo, config: EndpointConfig) -> Self {
        let server = wire.peer_addr();
        Self::client(
            EmulatedWires::client(wire),
            server,
            Box::new(PlainEstablishment::default()),
            config,
        )
    }

    /// A server's endpoint of plain sessions over the simulated network, which takes each
    /// client's wire as its host adds it.
    pub fn plain_server(config: EndpointConfig) -> Self {
        Self::server(
            EmulatedWires::server(),
            Box::new(PlainEstablishment::default()),
            config,
        )
    }
}
