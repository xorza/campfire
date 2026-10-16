use std::any::type_name;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_script::View;

use crate::role::Role;

/// Package data a module reads, which its declaration puts in a match empty, and the install
/// fills from the packages, each book through `Book::load_into`. The module that declares it is its
/// owner, which the install knows.
pub trait Book: Resource + Default {
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

    /// Puts the book in place of the empty one its owner's declaration put in `world`, and gives
    /// the script view what it copies of it: the step of the install for each book, which a test
    /// that loads a book alone takes too.
    fn load_into(self, world: &mut World) {
        assert!(
            world.contains_resource::<Self>(),
            "{} is declared by its owner",
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
