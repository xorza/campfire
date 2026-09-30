use bevy_ecs::component::Component;
use campfire_sim::{EntityIndex, SimComponent, StableId};
use serde::{Deserialize, Serialize};

/// Who struck a unit, and the last tick each did, by stable id. An attacker is forgotten once it
/// no longer exists, so the list never outgrows the units of the match.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecentAttackers(Vec<RecentAttack>);

/// The last tick `source` struck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentAttack {
    pub source: StableId,
    pub tick: u64,
}

impl RecentAttackers {
    pub fn iter(&self) -> impl Iterator<Item = RecentAttack> + '_ {
        self.0.iter().copied()
    }

    /// Records that `source` struck in `tick`, and forgets the attackers `index` no longer holds.
    pub(crate) fn record(&mut self, source: StableId, tick: u64, index: &EntityIndex) {
        self.0.retain(|attack| index.get(attack.source).is_some());
        match self.0.binary_search_by_key(&source, |attack| attack.source) {
            Ok(at) => self.0[at].tick = tick,
            Err(at) => self.0.insert(at, RecentAttack { source, tick }),
        }
    }
}

impl SimComponent for RecentAttackers {
    const NAME: &'static str = "combat.recent_attackers";
}
