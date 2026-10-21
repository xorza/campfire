use campfire_sim::Capability;
use campfire_values::UnitType;

use crate::fan::Fan;

/// How an action delivers, as a match runs it: the unit type it delivers, and its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivery {
    pub unit_type: UnitType,
    pub shape: DeliveryShape,
}

/// The shape of a delivery: a fan of projectiles, which fly as their type does, or an area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryShape {
    Projectile { fan: Fan, flight: ProjectileFlight },
    Area,
}

impl DeliveryShape {
    /// The capability of each shape, in the order of the shapes.
    pub const CAPABILITIES: [Capability; 2] = [Capability::Projectiles, Capability::Areas];

    /// The capability that delivers it.
    pub const fn capability(self) -> Capability {
        match self {
            Self::Projectile { .. } => Self::CAPABILITIES[0],
            Self::Area => Self::CAPABILITIES[1],
        }
    }
}

/// How a projectile type flies: along a line, which a script launches along a direction, or at a
/// unit, homing on it, on a curve to it or steered toward it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileFlight {
    Line,
    Homing,
    Curve,
    Steered,
}

impl ProjectileFlight {
    /// Whether a script launches it at a unit, not along a direction.
    pub const fn is_at_unit(self) -> bool {
        matches!(self, Self::Homing | Self::Curve | Self::Steered)
    }
}
