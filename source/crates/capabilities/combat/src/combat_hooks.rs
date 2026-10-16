use std::rc::Rc;

use bevy_ecs::world::World;
use campfire_declare::Role;
use campfire_script::Ctx;

use crate::modifier_hooks::ModifierHooks;

/// The modifiers that hear combat's events, none in a match that runs no script. A non-send
/// resource, as it holds the script `ctx`; a pass clones it, which shares the scripts, while it
/// runs.
#[derive(Debug, Default, Clone)]
pub struct CombatHooks {
    pub(crate) events: Option<Rc<ModifierHooks>>,
}

impl CombatHooks {
    /// The hooks as the install of `role`'s match finds them in `world`: the modifiers' events
    /// in a match that runs scripts, which hear them through its `ctx`.
    pub(crate) fn install(world: &World, role: &Role) -> Self {
        let events = role
            .scripts()
            .then(|| Rc::new(ModifierHooks::new(world.non_send::<Ctx>().clone())));
        Self { events }
    }
}
