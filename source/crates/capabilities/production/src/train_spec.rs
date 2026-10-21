use campfire_actions::PerAction;
use campfire_values::UnitType;

/// What a train action runs: the unit type it makes, and the types it makes in that type's
/// place, one drawn for each unit, when it has some.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainSpec {
    pub unit_type: UnitType,
    pub variants: Box<[UnitType]>,
}

impl PerAction for TrainSpec {
    const SCRIPTS_ONLY: bool = false;
}
