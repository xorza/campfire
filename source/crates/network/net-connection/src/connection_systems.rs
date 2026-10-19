use bevy_ecs::schedule::SystemSet;

/// The connection's two steps of the frame.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ConnectionSystems {
    /// Receive bytes from the Link, process them as Packets and buffer them into the Transport
    Receive,

    /// Flush the messages that were buffered into the Transport, process them as Packets and
    /// buffer them to the Link
    Send,
}
