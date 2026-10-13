use bevy_ecs::resource::Resource;
use campfire_common::Ticks;
use campfire_core::{Book, View};
use campfire_sim::Capability;

use crate::combat_bindings::CombatBindings;

/// What the mode's `[combat]` binds besides its life pool, and how long after its last strike an
/// attacker counts as assisting in a death, `assist_window_ms` in ticks: without it, no one
/// assists. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CombatBook {
    pub bindings: CombatBindings,
    pub assist_window: Option<Ticks>,
}

impl Book for CombatBook {
    const OWNER: Option<Capability> = Some(Capability::Combat);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View) {}
}
