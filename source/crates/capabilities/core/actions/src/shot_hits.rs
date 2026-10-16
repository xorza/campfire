use bevy_ecs::resource::Resource;
use campfire_declare::Scratch;
use campfire_sim::StableId;
use campfire_values::{Hit, Team};

use crate::reached::Reached;
use crate::shot_damage::ShotDamage;

/// The hits and the ends of the tick's attack projectiles, in the order they came, which the
/// capability that deals an attack's damage takes as the projectiles end their flight. Not
/// state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub struct ShotHits(pub Vec<ShotHit>);

/// A hit or the end of a ranged attack's projectile: `source`'s shot, of `team` as it fired,
/// reached what `reach` says, as `hit` says, with its `damage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShotHit {
    pub source: StableId,
    pub team: Team,
    pub reach: Reached,
    pub damage: ShotDamage,
    pub hit: Hit,
}

impl Scratch for ShotHits {
    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
