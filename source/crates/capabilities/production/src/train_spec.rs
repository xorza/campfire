use campfire_declarations::PerId;
use campfire_values::{ActionId, UnitType};

/// What a train action runs: the unit type it makes, and the types it makes in that type's
/// place, one drawn for each unit, when it has some.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainSpec {
    pub unit_type: UnitType,
    pub variants: Box<[UnitType]>,
}

impl PerId<ActionId> for TrainSpec {
    const SCRIPTS_ONLY: bool = false;
}
