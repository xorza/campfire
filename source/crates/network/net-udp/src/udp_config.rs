use campfire_net_emulator::LinkConditioning;
use campfire_net_packets::{EndpointConfig, SessionConfig};

/// What both ends of a UDP link agree on: the app's protocol, which a peer of another app does not
/// offer, so the two never link (13-net-stack/udp.md, A server's identity and a client's trust
/// match); how a quiet session proves it lives and how long a silent one lasts, which bounds a
/// handshake too; and the model applied to what the endpoint receives.
#[derive(Debug, Clone)]
pub struct UdpConfig {
    /// The ALPN of the app's protocol.
    pub alpn: Vec<u8>,
    pub session: SessionConfig,
    pub conditioner: Option<LinkConditioning>,
}

impl UdpConfig {
    /// The config of the app protocol `alpn`, with the session's defaults: a keep-alive each
    /// second and a loss after 5 s.
    pub fn new(alpn: impl Into<Vec<u8>>) -> Self {
        let alpn = alpn.into();
        assert!(!alpn.is_empty(), "a UDP link names its app's ALPN");
        Self {
            alpn,
            session: SessionConfig::default(),
            conditioner: None,
        }
    }

    pub(crate) fn endpoint(&self) -> EndpointConfig {
        EndpointConfig {
            session: self.session,
            conditioner: self.conditioner.clone(),
        }
    }
}
