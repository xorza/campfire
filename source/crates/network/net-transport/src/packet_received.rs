use bevy_ecs::prelude::*;
use campfire_net_core::NetTick;

/// Event triggered on a [`Transport`](crate::transport::Transport) entity when it receives a new
/// packet
#[derive(Debug, EntityEvent)]
pub struct PacketReceived {
    pub entity: Entity,
    pub remote_tick: NetTick,
}
