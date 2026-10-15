//! The query filters of the kinds of entity, so a system names a kind once, the way the kind's
//! markers say it.

use bevy_ecs::query::With;

use crate::unit_entity::UnitEntity;

/// Every unit, a delivery among them.
pub type UnitFilter = With<UnitEntity>;
