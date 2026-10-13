use bevy_ecs::resource::Resource;
use campfire_core::{ActionId, Scratch, UnitType};
use campfire_math::Num;
use campfire_sim::{Position, StableId};
use campfire_values::{DamageKind, Rank};

/// The projectiles the tick's ranged attacks fire, delivery requests that `projectiles` launches
/// in `Phase::Fire`. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub struct Shots(pub Vec<Shot>);

/// The projectile of a ranged attack: of `unit_type`, from `source` at `from`, homing on
/// `target`, with its weapon `action`'s damage of `kind`, the rank of the weapon's slot, and the
/// roll it drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    pub source: StableId,
    pub from: Position,
    pub target: StableId,
    pub unit_type: UnitType,
    pub action: ActionId,
    pub rank: Rank,
    pub amount: Num,
    pub kind: DamageKind,
    pub roll: Num,
}

impl Scratch for Shots {
    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
