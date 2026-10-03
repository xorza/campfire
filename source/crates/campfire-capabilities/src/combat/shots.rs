use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::{Position, StableId};

use crate::units::action_id::ActionId;
use crate::units::unit_type::UnitType;
use crate::values::damage_kind::DamageKind;

/// The projectiles the tick's ranged attacks fire, which `projectiles` launches in
/// `CombatSet::Fire`. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct Shots(pub(crate) Vec<Shot>);

/// The projectile of a ranged attack: of `unit_type`, from `source` at `from`, homing on
/// `target`, with its weapon `action`'s damage of `kind`, the rank of the weapon's slot, and the
/// roll it drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shot {
    pub(crate) source: StableId,
    pub(crate) from: Position,
    pub(crate) target: StableId,
    pub(crate) unit_type: UnitType,
    pub(crate) action: ActionId,
    pub(crate) rank: u8,
    pub(crate) amount: Num,
    pub(crate) kind: DamageKind,
    pub(crate) roll: Num,
}
