use crate::actions::fan::Fan;
use crate::units::unit_type::UnitType;

/// How an action delivers, as a match runs it: the unit type it delivers, and its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Delivery {
    pub(crate) unit_type: UnitType,
    pub(crate) shape: DeliveryShape,
}

/// The shape of a delivery: a fan of projectiles, which home on a unit when their type does, or
/// an area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryShape {
    Projectile { fan: Fan, homes: bool },
    Area,
}
