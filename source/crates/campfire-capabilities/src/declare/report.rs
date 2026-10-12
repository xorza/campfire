use bevy_ecs::resource::Resource;

/// A tick's record for the host, which the sim writes within a tick and the host reads after it:
/// no state, as the sim never reads it from an earlier tick. Its module puts it in.
pub(crate) trait Report: Resource {}
