use campfire_actions::PerAction;
use campfire_values::UnitType;

/// What a train action runs: the unit type it makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainSpec {
    pub unit_type: UnitType,
}

impl PerAction for TrainSpec {
    const SCRIPTS_ONLY: bool = false;
}
