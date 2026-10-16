use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::damage::Damage;

/// The hits of slow weapons on their way, each with the tick it lands in, in the order shot
/// (20-zero-hour-projectiles.md, A slow weapon delays its damage). State, as a snapshot between
/// a shot and its hit holds them.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HeldHits(Vec<HeldHit>);

/// A hit on its way: the damage it deals, and the tick it lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct HeldHit {
    lands: Tick,
    damage: Damage,
}

impl HeldHits {
    /// How many hits are on their way.
    pub(crate) const fn len(&self) -> usize {
        self.0.len()
    }

    /// Holds `damage` until `lands`.
    pub(crate) fn hold(&mut self, lands: Tick, damage: Damage) {
        self.0.push(HeldHit { lands, damage });
    }

    /// Whether a hit lands by `now`, which `land` then moves out, so a tick with none changes
    /// nothing.
    pub(crate) fn lands_by(&self, now: Tick) -> bool {
        self.0.iter().any(|hit| hit.lands <= now)
    }

    /// Moves the damage of each hit that lands by `now` into `due`, in the order shot.
    pub(crate) fn land(&mut self, now: Tick, due: &mut Vec<Damage>) {
        self.0.retain(|hit| {
            let lands = hit.lands <= now;
            if lands {
                due.push(hit.damage);
            }
            !lands
        });
    }
}

impl SimResource for HeldHits {
    const NAME: &'static str = "combat.held_hits";

    // Each damage of an action the book holds and of a kind the mode declares, as the pass reads
    // them.
    fn check(&self, world: &World) -> bool {
        self.0.iter().all(|hit| hit.damage.known(world))
    }
}
