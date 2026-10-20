use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{EntityIndex, SimComponent, StableId};
use campfire_states::{NotSent, Replication};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::recent_attack::RecentAttack;

/// Who struck a unit, and the last tick each did, by stable id. An attacker that no longer exists
/// is forgotten as a new one comes, so the list never holds more than the units that lived since
/// then; each reader skips those gone.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RecentAttackers(Vec<RecentAttack>);

impl RecentAttackers {
    pub fn iter(&self) -> impl Iterator<Item = RecentAttack> + '_ {
        self.0.iter().copied()
    }

    /// Records that `source`, which `index` holds, struck in `tick`. A new attacker first makes
    /// it forget each attacker `index` no longer holds, so only a growing list pays for the
    /// search. A projectile can strike after its source is gone, so the caller records only a
    /// source that exists.
    pub fn record(&mut self, source: StableId, tick: Tick, index: &EntityIndex) {
        debug_assert!(index.get(source).is_some(), "a source that exists");
        if let Ok(at) = self.at(source) {
            self.0[at].tick = tick;
            return;
        }
        self.0.retain(|attack| index.get(attack.source).is_some());
        let at = self.0.partition_point(|attack| attack.source < source);
        self.0.insert(at, RecentAttack { source, tick });
    }

    /// Where `source` is in the list, or where it would go.
    fn at(&self, source: StableId) -> Result<usize, usize> {
        self.0.binary_search_by_key(&source, |attack| attack.source)
    }

    /// Forgets every attacker, as the unit comes back, and keeps the buffer.
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// A snapshot is untrusted, so attackers out of the order of their stable ids, or one twice, which
/// `record`'s search relies on, fail to decode.
impl<'de> Deserialize<'de> for RecentAttackers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let attacks = Vec::<RecentAttack>::deserialize(deserializer)?;
        if !attacks.is_sorted_by(|a, b| a.source < b.source) {
            return Err(D::Error::custom("attackers in order, each once"));
        }
        Ok(Self(attacks))
    }
}

impl SimComponent for RecentAttackers {
    const NAME: &'static str = "combat.recent_attackers";
    type Host = UnitEntity;

    // Each attacker may be gone, which every reader allows, and its decode keeps each tick
    // within the limit.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for RecentAttackers {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn attackers_decode_in_order_each_once() {
        let mut ids = IdAllocator::default();
        let [first, second] = [ids.allocate(), ids.allocate()].map(|source| RecentAttack {
            source,
            tick: Tick::new(4),
        });
        let decode = |attacks: &[RecentAttack]| {
            let bytes = Binary::encode(attacks);
            Binary::decode::<RecentAttackers>(&bytes).is_ok()
        };
        assert!(decode(&[]) && decode(&[first, second]));
        assert!(!decode(&[second, first]) && !decode(&[first, first]));
    }
}
