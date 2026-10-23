use campfire_declarations::PerId;
use campfire_values::{ActionId, UnitType};

/// What a train action runs: the unit type it makes, whose spawn draws its variation when it varies
/// (04-capabilities/00-overview.md, A unit type may vary its units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainSpec {
    pub unit_type: UnitType,
}

impl PerId<ActionId> for TrainSpec {
    const SCRIPTS_ONLY: bool = false;
}
