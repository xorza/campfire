use campfire_net_emulator::LinkConditioning;

use crate::session_config::SessionConfig;

/// How an endpoint's sessions run, beside its IO and its establishment.
#[derive(Debug, Clone, Default)]
pub struct EndpointConfig {
    pub session: SessionConfig,
    /// The model the endpoint applies to each datagram it receives, before its sessions see it,
    /// so their RTT, loss detection and window see the delay and loss it adds.
    pub conditioner: Option<LinkConditioning>,
}
