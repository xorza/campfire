use crate::client::Client;
use crate::client_of::ClientOf;
use bevy_ecs::component::Component;
use campfire_net_core::Direction;

/// A side of a connection, marked by its component: what it sends and what it receives travel
/// one way each. A [`Client`] sends toward its server, and a [`ClientOf`] toward its client.
pub trait LinkRole: Component {
    /// The direction this side's messages travel.
    const SENDS: Direction;
    /// The direction the messages this side receives travel.
    const RECEIVES: Direction;
}

impl LinkRole for Client {
    const SENDS: Direction = Direction::ClientToServer;
    const RECEIVES: Direction = Direction::ServerToClient;
}

impl LinkRole for ClientOf {
    const SENDS: Direction = Direction::ServerToClient;
    const RECEIVES: Direction = Direction::ClientToServer;
}
