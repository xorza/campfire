use bevy_ecs::resource::Resource;

/// A resource built again from the match's state, its map or its install, so a restore needs
/// none of it: no state. The restore tests, which compare a restored match's hashes with the
/// original's, hold that it is built again.
pub trait Cache: Resource + Sized {
    /// The cache as the install puts it in; none for one its module builds as the match loads,
    /// from its map.
    fn installed() -> Option<Self>;
}
