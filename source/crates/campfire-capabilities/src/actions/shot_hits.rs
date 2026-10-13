use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::StableId;

use crate::declare::scratch::Scratch;
use crate::units::action_id::ActionId;
use crate::values::damage_kind::DamageKind;
use crate::values::hit::Hit;
use crate::values::rank::Rank;

/// The hits of the tick's attack projectiles, in the order they struck, which the capability that
/// deals an attack's damage takes as the projectiles end their flight. Not state: it empties
/// within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct ShotHits(pub(crate) Vec<ShotHit>);

/// The hit of a ranged attack's projectile: `source`'s weapon `action` at `rank` struck `target`
/// as `hit` says, with its damage `amount` of `kind` and the roll it drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShotHit {
    pub(crate) source: StableId,
    pub(crate) target: StableId,
    pub(crate) action: ActionId,
    pub(crate) rank: Rank,
    pub(crate) amount: Num,
    pub(crate) kind: DamageKind,
    pub(crate) roll: Num,
    pub(crate) hit: Hit,
}

impl Scratch for ShotHits {
    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
