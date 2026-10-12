use bevy_ecs::resource::Resource;

/// A resource built again from the match's state, its map or its install, so a restore needs
/// none of it: no state. Its module builds it; the restore tests, which compare a restored
/// match's hashes with the original's, hold that it is built again.
pub(crate) trait Cache: Resource {}
