use bevy_ecs::schedule::SystemSet;

/// System sets for connection-related logic.
/// These are used to order systems that handle receiving and sending packets.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ConnectionSystems {
    /// Receive bytes from the Link, process them as Packets and buffer them into the Transport
    Receive,

    /// Flush the messages that were buffered into the Transport, process them as Packets and
    /// buffer them to the Link
    Send,
}
