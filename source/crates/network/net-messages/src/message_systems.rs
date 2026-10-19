use bevy_ecs::schedule::SystemSet;

/// System sets for the typed-message pipeline.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum MessageSystems {
    /// Receives bytes from the transport, deserializes them into the typed message receivers, and
    /// triggers received events.
    Receive,
    /// Serializes the messages of every [`MessageSender<M>`](crate::message_sender::MessageSender)
    /// and buffers them in the transport.
    Send,
}
