use campfire_sim::{Capability, Position};

use crate::deliveries::delivering::Delivering;
use crate::scripts::effects::Effect;
use crate::units::unit_type::UnitType;

/// An area a call queued: of `by`, of `unit_type`, at `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AreaEffect {
    pub(crate) by: Delivering,
    pub(crate) unit_type: UnitType,
    pub(crate) at: Position,
}

impl Effect for AreaEffect {
    const CAPABILITY: Capability = Capability::Areas;
}
