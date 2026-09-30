use bevy_ecs::component::Component;
use campfire_sim::{SimComponent, StableId};
use serde::{Deserialize, Serialize};

/// A unit's attack order and where its attack stands. The period runs from an attack's start,
/// but only a strike spends it: an attack cancelled in its windup leaves the unit ready.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackState {
    target: Option<StableId>,
    /// The tick the attack in its windup started in.
    started: Option<u64>,
    /// The first tick the next attack may start in.
    ready_at: u64,
}

impl AttackState {
    pub const fn target(self) -> Option<StableId> {
        self.target
    }

    pub const fn started(self) -> Option<u64> {
        self.started
    }

    pub const fn ready_at(self) -> u64 {
        self.ready_at
    }

    /// Attacks `target` from now on; another target than the current one cancels a windup.
    pub(crate) fn set_target(&mut self, target: Option<StableId>) {
        if target != self.target {
            self.started = None;
        }
        self.target = target;
    }

    pub(crate) const fn start(&mut self, tick: u64) {
        self.started = Some(tick);
    }

    /// Ends the windup with its strike, which spends the period from the attack's start.
    pub(crate) const fn strike(&mut self, ready_at: u64) {
        self.started = None;
        self.ready_at = ready_at;
    }
}

impl SimComponent for AttackState {
    const NAME: &'static str = "moba.attack";
}
