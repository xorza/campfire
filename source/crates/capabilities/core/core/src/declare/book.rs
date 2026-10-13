use std::any::type_name;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::Capability;

use crate::declare::capability_set::CapabilitySet;
use crate::scripts::ctx::Ctx;
use crate::scripts::frame::Frame;
use crate::units::view::View;

/// Package data a module reads, which its declaration puts in a match empty, and the install
/// fills from the packages, each book through `Book::load_into`. The module that declares it is its
/// owner: the declaration audit holds the two to agree.
pub trait Book: Resource + Default {
    /// The capability that declares it; none for the core.
    const OWNER: Option<Capability>;
    /// Whether only a match that runs scripts reads it, so a client holds none.
    const SCRIPTS_ONLY: bool;

    /// Gives the script view and, in a match that runs scripts, the call frame what they copy
    /// of it.
    fn share(&self, view: &View, frame: Option<&mut Frame>);

    /// Whether a match of the capabilities `set`, which runs scripts when `scripts` says, holds
    /// it: one of its owner's, as it reads it.
    fn held(set: CapabilitySet, scripts: bool) -> bool {
        set.runs(Self::OWNER) && (scripts || !Self::SCRIPTS_ONLY)
    }

    /// Puts the book in place of the empty one its owner's declaration put in `world`, and gives
    /// the script view and, in a match that runs scripts, the call frame what they copy of it:
    /// the step of the install for each book, which a test that loads a book alone takes too.
    fn load_into(self, world: &mut World) {
        assert!(
            world.contains_resource::<Self>(),
            "{} is declared by its owner",
            type_name::<Self>()
        );
        world.insert_resource(self);
        Self::share_in(world);
    }

    /// Gives the script view and, in a match that runs scripts, the call frame what they copy of
    /// `world`'s book, as it stands.
    fn share_in(world: &World) {
        let view = world.non_send::<View>();
        let ctx = world.get_non_send::<Ctx>();
        let mut frame = ctx.map(Ctx::frame);
        world.resource::<Self>().share(view, frame.as_deref_mut());
    }
}
