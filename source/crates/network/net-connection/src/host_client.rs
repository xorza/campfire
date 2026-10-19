use bevy_ecs::prelude::*;
use bytes::Bytes;
use campfire_net_core::Direction;
use campfire_net_core::NetId;
use campfire_net_core::NetTick;

/// Marker component inserted on a client that acts as a Host
#[derive(Component, Debug)]
pub struct HostClient {
    /// The messages its two sides sent each other, which skip the transport.
    pub buffer: Vec<HostPayload>,
}

/// One message a side of a host client sent the other.
#[derive(Debug)]
pub struct HostPayload {
    pub bytes: Bytes,
    /// The net id of the channel the message was sent on.
    pub channel: NetId,
    /// The app's tick when the message was sent.
    pub tick: NetTick,
    /// Toward the host client, from the server's link to it, or toward the server, from the
    /// host client.
    pub direction: Direction,
}
