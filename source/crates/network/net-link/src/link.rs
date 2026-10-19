use bevy_ecs::prelude::*;
use campfire_net_payload::PacketSessionQueues;

/// Transport-neutral link between two peers.
///
/// It holds the queues the frame and the link's packet session hand each other; the packet session
/// and its IO live on the endpoint (13-net-stack/packet-session.md, The seam is a set of queues on
/// `Link`).
#[derive(Debug, Component, Default)]
pub struct Link {
    /// What the frame and the link's packet session hand each other.
    pub packet_session: PacketSessionQueues,
}
