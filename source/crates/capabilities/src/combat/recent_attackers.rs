use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Tick;
use campfire_sim::{EntityIndex, SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::units::recent_attack::RecentAttack;

/// Who struck a unit, and the last tick each did, by stable id. An attacker is forgotten once it
/// no longer exists, so the list never outgrows the units of the match.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecentAttackers(Vec<RecentAttack>);

impl RecentAttackers {
    pub fn iter(&self) -> impl Iterator<Item = RecentAttack> + '_ {
        self.0.iter().copied()
    }

    /// Records that `source` struck in `tick`, when it still exists, and forgets the attackers
    /// `index` no longer holds. A projectile can strike after its source is gone.
    pub(crate) fn record(&mut self, source: StableId, tick: Tick, index: &EntityIndex) {
        self.0.retain(|attack| index.get(attack.source).is_some());
        if index.get(source).is_none() {
            return;
        }
        match self.0.binary_search_by_key(&source, |attack| attack.source) {
            Ok(at) => self.0[at].tick = tick,
            Err(at) => self.0.insert(at, RecentAttack { source, tick }),
        }
    }
}

impl SimComponent for RecentAttackers {
    const NAME: &'static str = "combat.recent_attackers";

    // Each attacker may be gone, which every reader allows.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}
