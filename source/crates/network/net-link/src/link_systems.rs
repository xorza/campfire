use bevy_ecs::prelude::*;

/// The sets of the packet sessions' frame steps, which the transport orders itself against.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum LinkSystems {
    /// Each packet session hands its link's
    /// [`PacketSessionQueues`](campfire_net_payload::PacketSessionQueues) what it opened, the fates
    /// of what it sent, and its budget.
    Receive,

    /// Each packet session takes what the frame queued on its link's
    /// [`PacketSessionQueues`](campfire_net_payload::PacketSessionQueues).
    Send,
}
