use campfire_script::rhai::ImmutableString;
use campfire_sim::{Position, StableId, Ticks};

use crate::mode::match_end::MatchResult;
use crate::navigation::path_walker::PathEnd;
use crate::scripts::state_value::StateValue;
use crate::units::path_id::PathId;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::attitude::Attitude;

/// A change to the match that a mode call queued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModeEffect {
    Timer {
        name: String,
        ticks: Ticks,
        repeat: bool,
        data: Option<StateValue>,
    },
    /// At the markers of the tag.
    SpawnAvatars(ImmutableString),
    End(MatchResult),
    SpawnUnit {
        unit_type: UnitType,
        team: Team,
        pos: Position,
    },
    SpawnGroup {
        team: Team,
        path: PathId,
        from: PathEnd,
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
    SetRelation {
        a: Team,
        b: Team,
        attitude: Attitude,
    },
}
