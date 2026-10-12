use bevy_ecs::resource::Resource;
use campfire_common::Ticks;
use campfire_sim::Capability;

use crate::combat::combat_bindings::CombatBindings;
use crate::declare::book::Book;
use crate::scripts::frame::Frame;
use crate::units::view::View;

/// What the mode's `[combat]` binds besides its life pool, and how long after its last strike an
/// attacker counts as assisting in a death, `assist_window_ms` in ticks: without it, no one
/// assists. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CombatBook {
    pub(crate) bindings: CombatBindings,
    pub(crate) assist_window: Option<Ticks>,
}

impl Book for CombatBook {
    const OWNER: Option<Capability> = Some(Capability::Combat);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View, _: Option<&mut Frame>) {}
}
