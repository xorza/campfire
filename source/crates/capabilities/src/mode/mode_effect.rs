use campfire_math::{PlayerSlot, Ticks};
use campfire_sim::{Capability, StableId};

use crate::actions::action_book::ActionId;
use crate::actions::slot_kind::SlotKind;
use crate::mode::match_end::MatchResult;
use crate::mode::mode_book::{GroupUnit, SpawnAt};
use crate::navigation::path_walker::PathEnd;
use crate::scripts::effects::Effect;
use crate::scripts::state_value::StateValue;
use crate::units::path_id::PathId;
use crate::units::team::Team;
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
    End(MatchResult),
    /// Owned by the player, if the call names one.
    SpawnUnit {
        at: SpawnAt,
        owner: Option<PlayerSlot>,
    },
    SpawnGroup {
        team: Team,
        path: PathId,
        from: PathEnd,
        units: Vec<GroupUnit>,
    },
    Grant {
        unit: StableId,
        kind: SlotKind,
        abilities: Vec<ActionId>,
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

impl Effect for ModeEffect {
    const CAPABILITY: Capability = Capability::Mode;
}
