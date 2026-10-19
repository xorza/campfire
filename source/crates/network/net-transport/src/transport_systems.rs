use bevy_ecs::prelude::*;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum TransportSystems {
    /// Receive messages from the Link and buffer them into the receive channels.
    Receive,

    /// Flush the messages buffered in the send channels to the Link.
    Send,
}
