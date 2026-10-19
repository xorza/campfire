use core::net::SocketAddr;

use campfire_net_core::Random;
use campfire_net_emulator::EmulatedIo;
use campfire_net_packets::{EmulatedWires, EndpointPieces};

use crate::secure_establishment::client_side::ClientSide;
use crate::server_trust::ServerTrust;
use crate::udp_setup::UdpSetup;

/// A client of the UDP protocol: how it takes its server and what its link agrees on, over any
/// datagram IO, an OS socket's or the emulated network's, with the same handshake and seal.
#[derive(Debug, Clone)]
pub struct UdpClient {
    trust: ServerTrust,
    setup: UdpSetup,
}

impl UdpClient {
    pub const fn new(trust: ServerTrust, setup: UdpSetup) -> Self {
        Self { trust, setup }
    }

    /// The parts of the client's endpoint over `io`, toward `server`, which connects at its
    /// `LinkStart` and draws its connection IDs from `random`.
    pub fn endpoint<Io>(&self, io: Io, server: SocketAddr, random: Random) -> EndpointPieces<Io> {
        let establishment = ClientSide::new(
            self.trust.tls_config(&self.setup.alpn),
            self.trust.name(),
            self.setup.packet_session.idle_timeout,
            random,
        );
        EndpointPieces::client(io, server, Box::new(establishment), self.setup.endpoint())
    }

    /// The parts of the client's endpoint over the emulated network's `wire`, its random numbers
    /// from `seed`.
    pub fn emulated(&self, wire: EmulatedIo, seed: u64) -> EndpointPieces<EmulatedWires> {
        let server = wire.remote_address();
        self.endpoint(EmulatedWires::client(wire), server, Random::seeded(seed))
    }
}
