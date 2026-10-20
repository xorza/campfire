use campfire_common::{Secret, TlsKey};
use campfire_net_core::Random;
use campfire_net_packets::{EmulatedWires, EndpointPieces};

use crate::admission_setup::AdmissionSetup;
use crate::secure_establishment::server_side::ServerSide;
use crate::server_identity::ServerIdentity;
use crate::udp_setup::UdpSetup;

/// A server of the UDP protocol: what it presents and what its links agree on, over any datagram
/// IO, an OS socket's or the emulated network's, with the same handshake and seal.
#[derive(Debug)]
pub struct UdpServer {
    identity: ServerIdentity,
    setup: UdpSetup,
}

impl UdpServer {
    pub const fn new(identity: ServerIdentity, setup: UdpSetup) -> Self {
        Self { identity, setup }
    }

    /// The raw key the server presents, which its clients pin; `None` for a certificate.
    pub const fn tls_key(&self) -> Option<TlsKey> {
        self.identity.tls_key()
    }

    /// The parts of the server's endpoint over `io`, which admits each address within
    /// `admission` and draws its secrets and connection IDs from `random`.
    pub fn serve<Io>(
        self,
        io: Io,
        admission: AdmissionSetup,
        mut random: Random,
    ) -> EndpointPieces<Io> {
        let token_secret = Secret::new(random.array());
        let establishment = ServerSide::new(
            self.identity.tls_config(&self.setup.alpn),
            self.identity.reset_key(),
            &token_secret,
            admission,
            self.setup.packet_session.idle_timeout,
            random,
        );
        EndpointPieces::server(io, Box::new(establishment), self.setup.endpoint())
    }

    /// The parts of the server's endpoint on the emulated network, which takes each client's wire
    /// through its `io()`, its random numbers from `seed`, so a run of a seed repeats in all but
    /// the bytes of TLS's key shares, which TLS draws from the OS.
    pub fn emulated(self, admission: AdmissionSetup, seed: u64) -> EndpointPieces<EmulatedWires> {
        self.serve(EmulatedWires::server(), admission, Random::seeded(seed))
    }
}

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
