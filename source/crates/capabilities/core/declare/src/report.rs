use bevy_ecs::resource::Resource;

/// A tick's record for the host, which the sim writes within a tick and the host reads after it:
/// no state, as the sim never reads it from an earlier tick.
pub trait Report: Resource + Sized {
    /// The report as the install puts it in; none for one its module puts in as an event asks.
    fn installed() -> Option<Self>;
}
