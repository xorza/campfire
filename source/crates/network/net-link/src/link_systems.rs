use bevy_ecs::prelude::*;

/// The sets of the sessions' frame steps, which the transport orders itself against.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum LinkSystems {
    /// Each session hands its link's [`SessionQueues`](campfire_net_payload::SessionQueues)
    /// what it opened, the fates of what it sent, and its budget.
    Receive,

    /// Each session takes what the frame queued on its link's
    /// [`SessionQueues`](campfire_net_payload::SessionQueues).
    Send,
}
