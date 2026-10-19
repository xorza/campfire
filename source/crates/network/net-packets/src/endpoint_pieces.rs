use core::net::SocketAddr;

use campfire_net_emulator::EmulatedIo;

use crate::emulated_wires::EmulatedWires;
use crate::endpoint_setup::EndpointSetup;
use crate::establish::{ClientEstablish, ServerEstablish};
use crate::plain_establishment::PlainEstablishment;

/// What an endpoint is made of: its IO, the establishment that makes its packet sessions, its side
/// and its setup. An IO crate gives the parts, and a host builds its endpoint from them, so no IO
/// crate depends on the host (13-net-stack/packet-session.md, An IO crate gives the parts of an
/// endpoint).
#[derive(Debug)]
pub struct EndpointPieces<Io> {
    pub(crate) io: Io,
    pub(crate) side: EndpointSide,
    pub(crate) setup: EndpointSetup,
}

/// Whether an endpoint opens one packet session to its server or takes packet sessions, with the
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

impl<Io> EndpointPieces<Io> {
    /// A client's endpoint over `io`, toward `server`, whose packet sessions `establish` makes.
    pub fn client(
        io: Io,
        server: SocketAddr,
        establish: Box<dyn ClientEstablish>,
        setup: EndpointSetup,
    ) -> Self {
        Self {
            io,
            side: EndpointSide::Client { server, establish },
            setup,
        }
    }

    /// A server's endpoint over `io`, whose packet sessions `establish` makes.
    pub fn server(io: Io, establish: Box<dyn ServerEstablish>, setup: EndpointSetup) -> Self {
        Self {
            io,
            side: EndpointSide::Server { establish },
            setup,
        }
    }
}

impl EndpointPieces<EmulatedWires> {
    /// A client's endpoint of plain packet sessions over the emulated network's `wire`.
    pub fn plain_client(wire: EmulatedIo, setup: EndpointSetup) -> Self {
        let server = wire.remote_address();
        Self::client(
            EmulatedWires::client(wire),
            server,
            Box::new(PlainEstablishment::default()),
            setup,
        )
    }

    /// A server's endpoint of plain packet sessions over the emulated network, which takes each
    /// client's wire as its host adds it.
    pub fn plain_server(setup: EndpointSetup) -> Self {
        Self::server(
            EmulatedWires::server(),
            Box::new(PlainEstablishment::default()),
            setup,
        )
    }
}
