use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_script::MatchNames;

/// A resource built again from the match's state, its map or its install, so a restore needs
/// none of it: no state. The restore tests, which compare a restored match's hashes with the
/// original's, hold that it is built again.
pub trait Cache: Resource + Component<Mutability = Mutable> + Sized {
    /// The cache as the install puts it in, of the world it installs in, whose units a test may
    /// spawn before; none for one its module builds as the match loads, from its map.
    fn installed(world: &World) -> Option<Self>;

    /// Clears it as a load in place takes it, every unit despawned and the save's to restore: as
    /// the install put it in, or its module's load built it, in a world of no unit, so the match
    /// builds it again from the restore as a fresh restore does (08-sessions.md, A load goes back
    /// in place).
    fn clear(&mut self);
}

/// The install puts in no name: each book shares its own as it installs, and the mode its own as
/// it loads.
impl Cache for MatchNames {
    fn installed(_: &World) -> Option<Self> {
        Some(Self::default())
    }

    /// The names are the packages', which a load keeps.
    fn clear(&mut self) {}
}
