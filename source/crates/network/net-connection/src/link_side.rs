use bevy_ecs::component::Component;
use campfire_net_core::LinkDirection;

use crate::client::Client;
use crate::client_of::ClientOf;

/// A side of a connection, marked by its component: what it sends and what it receives travel
/// one way each. A [`Client`] sends toward its server, and a [`ClientOf`] toward its client.
pub trait LinkSide: Component {
    /// The direction this side's messages travel.
    const SENDS: LinkDirection;
    /// The direction the messages this side receives travel.
    const RECEIVES: LinkDirection;
}

impl LinkSide for Client {
    const SENDS: LinkDirection = LinkDirection::ClientToServer;
    const RECEIVES: LinkDirection = LinkDirection::ServerToClient;
}

impl LinkSide for ClientOf {
    const SENDS: LinkDirection = LinkDirection::ServerToClient;
    const RECEIVES: LinkDirection = LinkDirection::ClientToServer;
}
