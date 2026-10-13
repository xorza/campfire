use campfire_core::UnitType;

use crate::fan::Fan;

/// How an action delivers, as a match runs it: the unit type it delivers, and its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivery {
    pub unit_type: UnitType,
    pub shape: DeliveryShape,
}

/// The shape of a delivery: a fan of projectiles, which home on a unit when their type does, or
/// an area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryShape {
    Projectile { fan: Fan, homes: bool },
    Area,
}
