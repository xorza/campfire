use bevy_ecs::prelude::*;
use campfire_net_core::NetTick;

/// A packet that reached a [`Transport`](crate::transport::Transport) entity, with its tick.
#[derive(Debug, EntityEvent)]
pub struct PacketReceived {
    pub entity: Entity,
    pub remote_tick: NetTick,
}
