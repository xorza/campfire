use campfire_declarations::{ById, Fact};
use campfire_sim::Capability;
use campfire_values::UnitType;

use crate::fan::Fan;

/// How an action delivers, as a match runs it: the unit type it delivers, the capability that
/// sets it out, its fan, and how it aims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivery {
    pub unit_type: UnitType,
    pub capability: Capability,
    pub fan: Fan,
    pub aim: DeliveryAim,
}

/// How a delivery's unit aims, as its module gives it for its type: along a line, which a script
/// launches along a direction; at a unit alone; at a unit or a point it flies to; or placed where
/// its action aims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryAim {
    Along,
    AtUnit,
    AtUnitOrPoint,
    Placed,
}

impl DeliveryAim {
    /// Whether a script launches it at a unit, not along a direction.
    pub const fn is_at_unit(self) -> bool {
        matches!(self, Self::AtUnit | Self::AtUnitOrPoint)
    }

    /// Whether a shot of it at a point flies to that point, as a scatter aims it.
    pub const fn flies_to_point(self) -> bool {
        matches!(self, Self::AtUnitOrPoint)
    }

    /// Whether it flies to where it ends, as a splash goes off there.
    pub const fn flies(self) -> bool {
        !matches!(self, Self::Placed)
    }
}

/// How each delivery type aims, which each module that delivers fills for its types: what an
/// action that delivers one checks its aim by.
#[derive(Debug, Default)]
pub struct DeliveryAims(pub ById<UnitType, DeliveryAim>);

impl Fact for DeliveryAims {}
