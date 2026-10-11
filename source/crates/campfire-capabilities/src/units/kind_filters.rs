//! The query filters of the kinds of entity, so a system names a kind once, the way the kind's
//! markers say it.

use bevy_ecs::query::{With, Without};

use crate::units::delivery_entity::DeliveryEntity;
use crate::units::unit_entity::UnitEntity;

/// Every unit, a delivery among them.
pub type UnitFilter = With<UnitEntity>;

/// Every unit but a delivery: one a mode or a train spawns, with its body and its kit.
pub type BodyUnit = (With<UnitEntity>, Without<DeliveryEntity>);
