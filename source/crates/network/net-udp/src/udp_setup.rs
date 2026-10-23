use campfire_net_emulator::LinkConditioning;
use campfire_net_packets::{EndpointSetup, PacketSessionSetup};

/// What both ends of a UDP link agree on: the app's protocol, which a peer of another app does not
/// offer, so the two never link (11-net-stack/udp.md, A server's identity and a client's trust
/// match); how a quiet packet session proves it lives and how long a silent one lasts, which bounds
/// a handshake too; and the model applied to what the endpoint receives.
#[derive(Debug, Clone)]
pub struct UdpSetup {
    /// The ALPN of the app's protocol, of 1 to 255 bytes, as TLS carries it.
    pub(crate) alpn: Vec<u8>,
    pub packet_session: PacketSessionSetup,
    pub conditioner: Option<LinkConditioning>,
}

impl UdpSetup {
    /// The setup of the app protocol `alpn`, with the packet session's defaults: a keep-alive each
    /// second and a loss after 5 s.
    ///
    /// # Panics
    ///
    /// When `alpn` is empty or past 255 bytes, which TLS's protocol list holds no name of
    /// (RFC 7301 §3.1).
    pub fn new(alpn: impl Into<Vec<u8>>) -> Self {
        let alpn = alpn.into();
        assert!(
            (1..=255).contains(&alpn.len()),
            "a UDP link names its app's ALPN, of 1 to 255 bytes"
        );
        Self {
            alpn,
            packet_session: PacketSessionSetup::default(),
            conditioner: None,
        }
    }

    pub(crate) fn endpoint(&self) -> EndpointSetup {
        EndpointSetup {
            packet_session: self.packet_session,
            conditioner: self.conditioner.clone(),
        }
    }
}
