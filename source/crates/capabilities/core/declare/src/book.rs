use std::any::type_name;

use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::Schedules;
use bevy_ecs::world::World;
use campfire_script::View;
use campfire_sim::SimUpdate;

use crate::role::Role;

/// Package data a module reads, which its builder makes from the packages before the match is
/// built, and which the build puts in as its owner declares it, before its owner installs. The
/// module that declares it is its owner, which the install knows.
pub trait Book: Resource + Sized {
    /// Whether only a match that runs scripts reads it, so a client holds none.
    const SCRIPTS_ONLY: bool;

    /// Gives the script view what it copies of it: a call part or a view column takes its book
    /// from the match as it begins.
    fn share(&self, view: &View);

    /// Whether a match of `role`, which installs its owner, holds it: one that runs scripts, when
    /// only scripts read it.
    fn held(role: &Role) -> bool {
        role.scripts() || !Self::SCRIPTS_ONLY
    }

    /// Puts the book in place of the one the build put in `world`, as a test that loads a book
    /// alone does, and gives the script view what it copies of it, before the match's systems
    /// install, as their install reads the books (02-engine-core.md, A test's match installs at
    /// its first tick).
    fn load_into(self, world: &mut World) {
        assert!(
            world.contains_resource::<Self>(),
            "{} is declared by its owner",
            type_name::<Self>()
        );
        let built = world.get_resource::<Schedules>();
        assert!(
            !built.is_some_and(|schedules| schedules.contains(SimUpdate)),
            "{} loads before its match's systems install",
            type_name::<Self>()
        );
        world.insert_resource(self);
        Self::share_in(world);
    }

    /// Gives the script view what it copies of `world`'s book, as it stands.
    fn share_in(world: &World) {
        world.resource::<Self>().share(world.non_send::<View>());
    }
}
