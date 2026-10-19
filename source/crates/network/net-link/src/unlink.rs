use bevy_ecs::prelude::*;
use campfire_net_payload::UnlinkReason;

/// Entity event requesting that a transport terminate a [`Link`](crate::link::Link).
///
/// `LinkPlugin` observes this event and inserts [`Unlinked`](crate::link_state::Unlinked) with the
/// provided reason. Concrete transports can also observe it to close sockets, packet sessions,
/// streams, or in-process channels.
#[derive(EntityEvent, Clone, Debug)]
pub struct Unlink {
    /// Entity that holds the [`Link`](crate::link::Link) to terminate.
    #[event_target]
    pub entity: Entity,
    /// Structured reason propagated to [`Unlinked::reason`](crate::link_state::Unlinked::reason).
    pub reason: UnlinkReason,
}
