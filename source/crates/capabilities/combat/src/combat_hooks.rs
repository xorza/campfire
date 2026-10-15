use bevy_ecs::world::World;
use campfire_declare::Role;
use campfire_script::Ctx;

use crate::combat_event::CombatEvents;
use crate::damage::DamageWeigher;
use crate::heal::HealWeigher;
use crate::modifier_hooks::ModifierHooks;

/// The scripts combat's passes call, each none when no script defines it: the mode's
/// `calc_damage` and `calc_heal`, and the modifiers that hear combat's events. A non-send
/// resource, as each holds the script `ctx`; a pass clones it, which shares the scripts, while it
/// runs.
#[derive(Debug, Default, Clone)]
pub struct CombatHooks {
    pub weigher: Option<DamageWeigher>,
    pub healer: Option<HealWeigher>,
    pub(crate) events: Option<CombatEvents>,
}

impl CombatHooks {
    /// The hooks as the install of `role`'s match finds them in `world`: the modifiers' events
    /// in a match that runs scripts, which hear them through its `ctx`; no weigher, which the
    /// mode's load puts in.
    pub(crate) fn install(world: &World, role: &Role) -> CombatHooks {
        let events = role.scripts().then(|| {
            let hooks = ModifierHooks::new(world.non_send::<Ctx>().clone());
            CombatEvents::new(move |batch, event| hooks.hear(batch, event))
        });
        CombatHooks {
            events,
            ..CombatHooks::default()
        }
    }

    /// Whether any script runs in a pass.
    pub(crate) const fn scripted(&self) -> bool {
        self.weigher.is_some() || self.healer.is_some() || self.events.is_some()
    }
}
