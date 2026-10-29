use core::num::NonZeroU32;
use core::time::Duration;

use campfire_common::Secret;
use campfire_net_packets::PacketSessionSetup;

use crate::admission_setup::AdmissionSetup;
use crate::server_identity::ServerIdentity;
use crate::server_secret::ServerSecret;
use crate::server_trust::ServerTrust;
use crate::udp_client::UdpClient;
use crate::udp_server::UdpServer;
use crate::udp_setup::UdpSetup;

/// A link of the protocol for its tests: a server of one fixed raw key under a test ALPN, and a
/// client that pins it.
#[derive(Debug)]
pub struct TestLink;

impl TestLink {
    const ALPN: &'static [u8] = b"campfire-test";

    /// The setup of the test ALPN, with the packet session's defaults.
    pub fn setup() -> UdpSetup {
        UdpSetup::new(Self::ALPN)
    }

    /// Times short enough that an idle timeout passes inside a test: 4 keep-alives to it.
    pub fn short() -> UdpSetup {
        UdpSetup {
            packet_session: PacketSessionSetup {
                keep_alive: Duration::from_millis(50),
                idle_timeout: Duration::from_millis(200),
                bandwidth: None,
            },
            ..Self::setup()
        }
    }

    /// An admission that limits no address.
    pub const fn roomy() -> AdmissionSetup {
        AdmissionSetup::new(NonZeroU32::MAX, NonZeroU32::MAX, Duration::from_secs(1))
    }

    fn identity() -> ServerIdentity {
        ServerIdentity::raw_key(ServerSecret::new(Secret::new([7; 32])))
    }

    /// The test's server with `setup`.
    pub fn server(setup: UdpSetup) -> UdpServer {
        UdpServer::new(Self::identity(), setup)
    }

    /// A client with `setup` that pins the test's server.
    pub fn client(setup: UdpSetup) -> UdpClient {
        UdpClient::new(Self::pinned(), setup)
    }

    /// The trust that pins the test's server's raw key.
    pub fn pinned() -> ServerTrust {
        ServerTrust::pinned(Self::identity().tls_key().unwrap())
    }
}
