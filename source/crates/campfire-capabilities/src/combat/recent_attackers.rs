use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{EntityIndex, SimComponent, StableId};
use serde::{Deserialize, Serialize};

use crate::combat::recent_attack::RecentAttack;

/// Who struck a unit, and the last tick each did, by stable id. An attacker that no longer exists
/// is forgotten as a new one comes, so the list never holds more than the units that lived since
/// then; each reader skips those gone.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecentAttackers(Vec<RecentAttack>);

impl RecentAttackers {
    pub fn iter(&self) -> impl Iterator<Item = RecentAttack> + '_ {
        self.0.iter().copied()
    }

    /// Records that `source` struck in `tick`, when it still exists. A new attacker first makes
    /// it forget the attackers `index` no longer holds, so only a growing list pays for the
    /// search. A projectile can strike after its source is gone.
    pub(crate) fn record(&mut self, source: StableId, tick: Tick, index: &EntityIndex) {
        if index.get(source).is_none() {
            return;
        }
        if let Ok(at) = self.0.binary_search_by_key(&source, |attack| attack.source) {
            self.0[at].tick = tick;
            return;
        }
        self.0.retain(|attack| index.get(attack.source).is_some());
        let at = self.0.partition_point(|attack| attack.source < source);
        self.0.insert(at, RecentAttack { source, tick });
    }

    /// Forgets every attacker, as the unit comes back, and keeps the buffer.
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
}

impl SimComponent for RecentAttackers {
    const NAME: &'static str = "combat.recent_attackers";

    // Each attacker may be gone, which every reader allows.
    fn check(&self, _: &World, _: Entity) -> bool {
        self.0.iter().all(|attack| attack.tick <= Tick::LIMIT)
    }
}
