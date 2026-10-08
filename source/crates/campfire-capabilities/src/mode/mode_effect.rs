use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_sim::StableId;

use crate::actions::slot_kind::SlotKind;
use crate::mode::Mode;
use crate::mode::group_unit::GroupUnit;
use crate::mode::match_end::MatchResult;
use crate::mode::mode_book::ModeBook;
use crate::navigation::path_walker::PathEnd;
use crate::scripts::ctx::Ctx;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::scripts::state_value::StateValue;
use crate::units::action_id::ActionId;
use crate::units::path_id::PathId;
use crate::units::spawner::SpawnAt;
use crate::units::team::Team;
use crate::values::attitude::Attitude;
use crate::values::rank::Rank;

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
        /// The kind's first rank, which the actions take.
        rank: Option<Rank>,
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
    /// A save, at the end of the tick.
    Save,
}

/// By the match's mode.
impl Effect for ModeEffect {
    fn apply(self, world: &mut World, _: &mut Frame, now: Tick) {
        let ctx = world.non_send::<Ctx>().clone();
        let mode = ModeBook::of_match(&ctx);
        Mode::apply_effect(world, mode, now, self);
    }
}
