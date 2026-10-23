use std::num::NonZeroU32;
use std::time::Duration;

use campfire_common::Secret;
use campfire_net_core::TimeSource;
use campfire_net_core::TlsKey;
use campfire_net_emulator::{DuplexModel, EmulatedNetwork};
use campfire_net_endpoint::Endpoint;
use campfire_net_packets::EmulatedWires;
use campfire_net_udp::{
    AdmissionSetup, ServerIdentity, ServerSecret, ServerTrust, UdpClient, UdpServer, UdpSetup,
};
use campfire_protocol::{Entropy, Signer};

/// What each Campfire link takes of the stack's sealed UDP protocol: the ALPN `campfire`, a
/// server's identity from its own key, from which the stack derives its raw TLS key and its reset
/// key, so a key file keeps its published key, and an admission of the session's player slots from
/// each address (08-sessions.md, A raw key derived from the server's key; 08-sessions.md, Limits
/// for each address, from the session's player slots).
#[derive(Debug)]
pub struct UdpLinks;

/// A sealed link in the process: the server's end, for its endpoint, and the client's endpoint.
#[derive(Debug)]
pub(crate) struct LocalPair {
    pub(crate) server: Endpoint<EmulatedWires>,
    pub(crate) client: Endpoint<EmulatedWires>,
}

impl UdpLinks {
    pub const ALPN: &'static [u8] = b"campfire";

    /// The server whose own key `signer` holds.
    pub fn server(signer: &Signer) -> UdpServer {
        UdpServer::new(Self::identity(signer), Self::setup())
    }

    /// The identity of the server whose own key `signer` holds: the raw key derived from it.
    pub fn identity(signer: &Signer) -> ServerIdentity {
        ServerIdentity::raw_key(ServerSecret::new(Secret::new(*signer.secret_bytes())))
    }

    /// A client that pins the server's raw key `tls_key`.
    pub fn client(tls_key: TlsKey) -> UdpClient {
        UdpClient::new(ServerTrust::pinned(tls_key), Self::setup())
    }

    /// At most `player_slots` live links from one address, and `player_slots` new handshakes from
    /// it, which come back over `grace` (08-sessions.md, Limits for each address, from the
    /// session's player slots).
    pub const fn admission(player_slots: NonZeroU32, grace: Duration) -> AdmissionSetup {
        AdmissionSetup::new(player_slots, grace)
    }

    /// A sealed link between a server whose own key `signer` holds and its client in the same
    /// process, over the emulator on the OS clock with a perfect model, the stack's one in-process
    /// IO (08-sessions.md, One in-process link): the client pins the server's raw key, so
    /// the connect answer signs the link's channel binding.
    pub(crate) fn local(signer: &Signer, entropy: Entropy) -> LocalPair {
        let server = Self::server(signer);
        let client = Self::client(server.tls_key().expect("a raw key's identity"));
        // One draw for both ends, as a fixed entropy gives each draw the same bytes.
        let bytes = entropy.random();
        let seed = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().expect("8 bytes"));
        let wires = EmulatedNetwork::new(TimeSource::Os).pair(DuplexModel::perfect());
        // The one client of the process: no address to limit.
        let admission = Self::admission(NonZeroU32::MAX, Duration::from_secs(1));
        let server = Endpoint::from(server.emulated(admission, seed(0)));
        server.io().add(wires.server);
        let client = Endpoint::from(client.emulated(wires.client, seed(8)));
        LocalPair { server, client }
    }

    /// The protocol's setup with Campfire's ALPN, the stack's times kept.
    fn setup() -> UdpSetup {
        UdpSetup::new(Self::ALPN)
    }
}
