use campfire_sim::{Position, StableId, Ticks};

use crate::mode::match_end::MatchResult;
use crate::scripts::state_value::StateValue;
use crate::units::path_id::PathId;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;

/// A change to the match that a mode call queued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModeEffect {
    Timer {
        name: String,
        ticks: Ticks,
        repeat: bool,
        data: Option<StateValue>,
    },
    SpawnAvatars,
    End(MatchResult),
    SpawnUnit {
        unit_type: UnitType,
        team: Team,
        pos: Position,
    },
    SpawnGroup {
        team: Team,
        path: PathId,
        types: Vec<UnitType>,
    },
    Respawn {
        unit: StableId,
        ticks: Ticks,
    },
    Learn {
        unit: StableId,
        slot: u8,
    },
}
