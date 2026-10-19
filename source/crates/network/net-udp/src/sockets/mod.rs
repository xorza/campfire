//! The endpoints of the UDP protocol: on an OS socket, or on the emulated network with the same
//! handshake.

use core::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use campfire_net_core::Random;
use campfire_net_emulator::EmulatedIo;
use campfire_net_packets::{DatagramIo, EmulatedWires, EndpointPieces};

use crate::admission_setup::AdmissionSetup;
use crate::bind_error::BindError;
use crate::secure_establishment::client_side::ClientSide;
use crate::secure_establishment::server_side::ServerSide;
use crate::server_identity::ServerIdentity;
use crate::server_trust::ServerTrust;
use crate::udp_io::UdpIo;
use crate::udp_port::UdpPort;
use crate::udp_setup::UdpSetup;
use campfire_common::{Secret, TlsKey};

/// A server's socket, bound, and what its endpoint takes: it serves once its admission is known,
/// so no server's endpoint exists without one.
#[derive(Debug)]
pub struct ServerSocket {
    io: UdpIo,
    tls: Arc<rustls::ServerConfig>,
    identity: ServerIdentity,
    setup: UdpSetup,
}

impl ServerSocket {
    /// A server's socket on `address`, which presents `identity`. It binds now, before its app
    /// runs, so a port in use is this error, not a server with no link.
    pub fn bind(
        address: SocketAddr,
        identity: ServerIdentity,
        setup: UdpSetup,
    ) -> Result<Self, BindError> {
        let io = bind(address)?;
        Ok(Self {
            io,
            tls: identity.tls_config(&setup.alpn),
            identity,
            setup,
        })
    }

    /// The address the socket is bound to, its port the OS's when the address named 0.
    pub fn local_address(&self) -> SocketAddr {
        self.io.local_address()
    }

    /// The raw key the server presents, which its clients pin; `None` for a certificate.
    pub const fn tls_key(&self) -> Option<TlsKey> {
        self.identity.tls_key()
    }

    /// The parts of the server's endpoint, which admits each address within `admission`.
    pub fn serve(self, admission: AdmissionSetup) -> EndpointPieces<UdpIo> {
        let Self {
            io,
            tls,
            identity,
            setup,
        } = self;
        let mut random = Random::os();
        let token_secret = Secret::new(random.array());
        let establishment = ServerSide::new(
            tls,
            identity.reset_key(),
            &token_secret,
            admission,
            setup.packet_session.idle_timeout,
            random,
        );
        EndpointPieces::server(io, Box::new(establishment), setup.endpoint())
    }
}

/// A client's socket, bound to a port of the OS's on the unspecified address of its server's
/// family.
#[derive(Debug)]
pub struct ClientSocket {
    io: UdpIo,
    server: SocketAddr,
    trust: ServerTrust,
    setup: UdpSetup,
}

impl ClientSocket {
    /// A client's socket toward `server`, which it takes by `trust`.
    pub fn bind(
        server: SocketAddr,
        trust: ServerTrust,
        setup: UdpSetup,
    ) -> Result<Self, BindError> {
        let unspecified = if server.is_ipv4() {
            Ipv4Addr::UNSPECIFIED.into()
        } else {
            Ipv6Addr::UNSPECIFIED.into()
        };
        Ok(Self {
            io: bind(SocketAddr::new(unspecified, 0))?,
            server,
            trust,
            setup,
        })
    }

    pub fn local_address(&self) -> SocketAddr {
        self.io.local_address()
    }

    /// The parts of the client's endpoint, which connects at its `LinkStart`.
    pub fn endpoint(self) -> EndpointPieces<UdpIo> {
        let Self {
            io,
            server,
            trust,
            setup,
        } = self;
        let establishment = client_side(&trust, &setup, Random::os());
        EndpointPieces::client(io, server, Box::new(establishment), setup.endpoint())
    }
}

/// A server of the UDP protocol on the emulated network: the same handshake and seal as on a
/// socket, its own random numbers from a seed, so a run of a seed repeats in all but the bytes
/// of TLS's key shares, which TLS draws from the OS.
#[derive(Debug)]
pub struct EmulatedServer {
    identity: ServerIdentity,
    setup: UdpSetup,
    seed: u64,
}

impl EmulatedServer {
    pub const fn new(identity: ServerIdentity, setup: UdpSetup, seed: u64) -> Self {
        Self {
            identity,
            setup,
            seed,
        }
    }

    /// The parts of the server's endpoint, which takes each client's wire through its `io()` and
    /// admits each address within `admission`.
    pub fn serve(self, admission: AdmissionSetup) -> EndpointPieces<EmulatedWires> {
        let mut random = Random::seeded(self.seed);
        let token_secret = Secret::new(random.array());
        let establishment = ServerSide::new(
            self.identity.tls_config(&self.setup.alpn),
            self.identity.reset_key(),
            &token_secret,
            admission,
            self.setup.packet_session.idle_timeout,
            random,
        );
        EndpointPieces::server(
            EmulatedWires::server(),
            Box::new(establishment),
            self.setup.endpoint(),
        )
    }
}

/// The parts of a client's endpoint of the UDP protocol over the emulated network's `wire`, which
/// takes its server by `trust`, its random numbers from `seed`.
pub fn emulated_client(
    wire: EmulatedIo,
    trust: &ServerTrust,
    setup: &UdpSetup,
    seed: u64,
) -> EndpointPieces<EmulatedWires> {
    let server = wire.remote_address();
    let establishment = client_side(trust, setup, Random::seeded(seed));
    EndpointPieces::client(
        EmulatedWires::client(wire),
        server,
        Box::new(establishment),
        setup.endpoint(),
    )
}

fn client_side(trust: &ServerTrust, setup: &UdpSetup, random: Random) -> ClientSide {
    ClientSide::new(
        trust.tls_config(&setup.alpn),
        trust.name(),
        setup.packet_session.idle_timeout,
        random,
    )
}

fn bind(address: SocketAddr) -> Result<UdpIo, BindError> {
    let error = |error| BindError { address, error };
    let port = UdpPort::bind(address).map_err(error)?;
    let local = port.local_address().map_err(error)?;
    Ok(UdpIo::new(port, local))
}

#[cfg(test)]
mod tests;
