use campfire_math::Num;
use campfire_values::{ActionId, DamageKind, Rank};
use serde::{Deserialize, Serialize};

/// What a ranged attack's projectile carries to its hit: its weapon `action`, which its damage
/// names, at the rank of the weapon's slot, its damage `amount` of `kind`, and the roll it drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShotDamage {
    pub action: ActionId,
    pub rank: Rank,
    pub amount: Num,
    pub kind: DamageKind,
    pub roll: Num,
}
