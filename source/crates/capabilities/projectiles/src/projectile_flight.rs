use campfire_actions::DeliveryAim;

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
    /// How an action that delivers it aims: along a line, at a unit it homes on, or at a unit or
    /// a point it flies to.
    pub const fn aim(self) -> DeliveryAim {
        match self {
            Self::Line => DeliveryAim::Along,
            Self::Homing => DeliveryAim::AtUnit,
            Self::Curve | Self::Steered => DeliveryAim::AtUnitOrPoint,
        }
    }
}
