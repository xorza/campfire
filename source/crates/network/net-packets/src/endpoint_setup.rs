use campfire_net_emulator::LinkConditioning;

use crate::packet_session_setup::PacketSessionSetup;

/// How an endpoint's packet sessions run, beside its IO and its establishment.
#[derive(Debug, Clone, Default)]
pub struct EndpointSetup {
    pub packet_session: PacketSessionSetup,
    /// The model the endpoint applies to each datagram it receives, before its packet sessions see
    /// it, so their RTT, loss detection and window see the delay and loss it adds.
    pub conditioner: Option<LinkConditioning>,
}
