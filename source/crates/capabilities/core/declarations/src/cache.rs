use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_script::MatchNames;

/// A resource built again from the match's state, its map or its install, so a restore needs
/// none of it: no state. The restore tests, which compare a restored match's hashes with the
/// original's, hold that it is built again.
pub trait Cache: Resource + Sized {
    /// The cache as the install puts it in, of the world it installs in, whose units a test may
    /// spawn before; none for one its module builds as the match loads, from its map.
    fn installed(world: &World) -> Option<Self>;
}

/// The install puts in no name: each book shares its own as it installs, and the mode its own as
/// it loads.
impl Cache for MatchNames {
    fn installed(_: &World) -> Option<Self> {
        Some(Self::default())
    }
}
