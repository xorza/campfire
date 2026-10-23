use bevy_ecs::prelude::*;
use campfire_net_core::TickInstant;

/// The packets of a frame that reached a [`Transport`](crate::transport::Transport) entity's
/// channels, by the latest sender instant they carry: one for each link and frame that admitted
/// one.
#[derive(Debug, EntityEvent)]
pub struct PacketReceived {
    pub entity: Entity,
    pub remote: TickInstant,
}
