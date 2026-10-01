use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::{Position, StableId};

use crate::combat::damage_kind::DamageKind;

/// The ranged attacks that fire this tick. In a match with projectiles, a ranged attack fires
/// one at the end of its windup instead of striking; `projectiles` inserts this list and takes
/// its launches in `CombatSet::Launch`. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct Launches(pub(crate) Vec<Launch>);

/// An attack that fires a projectile from `from` at `target`, flying `speed` a tick, to strike
/// for `amount` of `kind`, with the roll its attack drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Launch {
    pub(crate) source: StableId,
    pub(crate) from: Position,
    pub(crate) target: StableId,
    pub(crate) amount: Num,
    pub(crate) kind: DamageKind,
    pub(crate) speed: Num,
    pub(crate) roll: Num,
}
