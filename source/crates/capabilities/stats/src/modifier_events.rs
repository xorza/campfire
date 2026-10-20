use bevy_ecs::world::World;
use campfire_declarations::Role;
use campfire_script::Ctx;

use crate::modifier_hooks::ModifierHooks;

/// The hooks that answer the events of the modifiers units hold, none in a match that runs no
/// script. A non-send resource, as it holds the script `ctx`; a pass clones its hooks, which
/// shares the scripts, while it runs.
#[derive(Debug, Default, Clone)]
pub struct ModifierEvents {
    hooks: Option<ModifierHooks>,
}

impl ModifierEvents {
    /// The hooks as the install of `role`'s match finds them in `world`: the modifiers' scripts
    /// in a match that runs scripts, which hear their events through its `ctx`.
    pub(crate) fn install(world: &World, role: &Role) -> Self {
        let hooks = role
            .scripts()
            .then(|| ModifierHooks::new(world.non_send::<Ctx>().clone()));
        Self { hooks }
    }

    /// The hooks of `world`'s match, none when it runs no script.
    pub fn of(world: &World) -> Option<ModifierHooks> {
        world.non_send::<Self>().hooks.clone()
    }
}
