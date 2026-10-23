use bevy_ecs::observer::On;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::ResMut;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::SimResource;
use campfire_units::Died;
use serde::{Deserialize, Serialize};

use crate::damage::Damage;

/// The hits of slow weapons on their way, each with the tick it lands in, by that tick, then in
/// the order shot (04-capabilities/combat.md, A slow weapon delays its damage), so a tick reads
/// only the hits that land in it. State, as a snapshot between a shot and its hit holds them.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HeldHits(Vec<HeldHit>);

/// A hit on its way: the damage it deals, and the tick it lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct HeldHit {
    lands: Tick,
    damage: Damage,
}

impl HeldHits {
    /// Holds `damage` until `lands`, after every hit that lands by then.
    pub(crate) fn hold(&mut self, lands: Tick, damage: Damage) {
        let at = self.0.partition_point(|hit| hit.lands <= lands);
        self.0.insert(at, HeldHit { lands, damage });
    }

    /// Whether a hit lands by `now`, which `land` then moves out, so a tick with none changes
    /// nothing.
    pub(crate) fn lands_by(&self, now: Tick) -> bool {
        self.0.first().is_some_and(|hit| hit.lands <= now)
    }

    /// Drops each hit on the unit `died` names, which it can no more land on, as a respawn would
    /// put the same unit back first.
    pub(crate) fn drop_died(died: On<'_, '_, Died>, held: Option<ResMut<'_, Self>>) {
        if let Some(mut held) = held {
            held.0.retain(|hit| hit.damage.target != died.id);
        }
    }

    /// Moves the damage of each hit that lands by `now` into `due`, by the tick it lands in, then
    /// in the order shot.
    pub(crate) fn land(&mut self, now: Tick, due: &mut Vec<Damage>) {
        let landed = self.0.partition_point(|hit| hit.lands <= now);
        due.extend(self.0.drain(..landed).map(|hit| hit.damage));
    }
}

impl SimResource for HeldHits {
    const NAME: &'static str = "combat.held_hits";

    // Each damage of an action the book holds and of a kind the mode declares, as the pass reads
    // them, by the tick it lands in, as a tick reads the first alone.
    fn check(&self, world: &World) -> bool {
        self.0.is_sorted_by_key(|hit| hit.lands)
            && self.0.iter().all(|hit| hit.damage.is_known(world))
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::held_hits::HeldHits;

    impl HeldHits {
        /// How many hits are on their way.
        pub(crate) const fn len(&self) -> usize {
            self.0.len()
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Num;
    use campfire_sim::IdAllocator;
    use campfire_values::DamageKind;

    use super::*;
    use crate::damage::DamageCause;

    #[test]
    fn hits_land_by_their_tick_then_in_the_order_shot() {
        // Hits of 1 to 4 shot to land in ticks 5, 3, 5 and 4: by tick 2 none lands, by tick 4
        // the hit of 2, then that of 4, and by tick 5 the hits of 1 and 3, as shot.
        let target = IdAllocator::default().allocate();
        let damage = |amount| Damage {
            source: None,
            target,
            amount: Num::int(amount),
            kind: DamageKind::new(0),
            cause: DamageCause::Effect,
            action: None,
            depth: 0,
            hit: None,
        };
        let mut held = HeldHits::default();
        for (lands, amount) in [(5, 1), (3, 2), (5, 3), (4, 4)] {
            held.hold(Tick::new(lands), damage(amount));
        }
        let landed = |held: &mut HeldHits, now| {
            let mut due = Vec::new();
            held.land(Tick::new(now), &mut due);
            due.iter().map(|damage| damage.amount).collect::<Vec<_>>()
        };
        assert!(!held.lands_by(Tick::new(2)));
        assert!(held.lands_by(Tick::new(3)));
        assert_eq!(landed(&mut held, 4), [Num::int(2), Num::int(4)]);
        assert!(!held.lands_by(Tick::new(4)));
        assert_eq!(landed(&mut held, 5), [Num::int(1), Num::int(3)]);
        assert_eq!(held, HeldHits::default());
    }
}
