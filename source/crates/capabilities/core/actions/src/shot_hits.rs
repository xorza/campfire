use bevy_ecs::resource::Resource;
use campfire_declare::Scratch;
use campfire_sim::StableId;
use campfire_values::Hit;

use crate::shot_damage::ShotDamage;

/// The hits of the tick's attack projectiles, in the order they struck, which the capability that
/// deals an attack's damage takes as the projectiles end their flight. Not state: it empties
/// within the tick.
#[derive(Resource, Debug, Default)]
pub struct ShotHits(pub Vec<ShotHit>);

/// The hit of a ranged attack's projectile: `source`'s shot struck `target` as `hit` says, with
/// its `damage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShotHit {
    pub source: StableId,
    pub target: StableId,
    pub damage: ShotDamage,
    pub hit: Hit,
}

impl Scratch for ShotHits {
    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
