use bevy_ecs::prelude::*;
use campfire_net_payload::SessionQueues;

/// Transport-neutral link between two peers.
///
/// `Link` is an ECS component owned by the entity that represents a local endpoint or a remote
/// peer. It holds the queues the frame and the link's session hand each other; the session and
/// its IO live on the endpoint.
#[derive(Debug, Component, Default)]
pub struct Link {
    /// What the frame and the link's session hand each other.
    pub session: SessionQueues,
}
