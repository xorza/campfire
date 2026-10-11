use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, With};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::Query;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{SimComponent, SimEdge, SimSet, Unpredicted};
use serde::{Deserialize, Serialize};

use crate::players::match_entity::MatchEntity;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;
use crate::units::team::Team;
use crate::units::view::View;

/// The end of a match: the tick whose stage ended it, and its result, on the match's entity. It
/// is state, so the final state hash a verifier checks holds the result too; once it exists, no
/// later stage runs, nor any pass between two stages but the closing set of the stage that ended
/// it. Immutable: a match ends once.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct MatchEnd {
    tick: Tick,
    result: MatchResult,
}

/// A stage with its closing set, which the match end stops together: the closing set belongs to
/// the stage before it, so it runs when its stage ran, the stage that ends the match included.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StageSpan(SimSet);

/// How a match ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchResult {
    Won(Team),
    Draw,
}

impl MatchEnd {
    pub(crate) const fn new(tick: Tick, result: MatchResult) -> MatchEnd {
        MatchEnd { tick, result }
    }

    /// The end of the match in `world`, once it ended.
    pub fn of(world: &World) -> Option<MatchEnd> {
        world.get(MatchEntity::entity(world)?).copied()
    }

    pub const fn tick(self) -> Tick {
        self.tick
    }

    pub const fn result(self) -> MatchResult {
        self.result
    }

    /// Stops every stage of `schedule` once a match ended, with its closing set, and
    /// `SimEdge::Start`, from the next stage on: each checks for the end once, as it starts, so
    /// the stage that ends the match is the last to run, its closing set with it. The tick still
    /// counts on, so a log may hold ticks after the end, which change nothing.
    pub(crate) fn stop_stages(schedule: &mut Schedule) {
        schedule.configure_sets(SimEdge::Start.run_if(MatchEnd::running));
        for stage in SimSet::ALL {
            let span = StageSpan(stage);
            schedule.configure_sets((
                stage.in_set(span),
                SimEdge::After(stage).in_set(span),
                span.run_if(MatchEnd::running),
            ));
        }
    }

    /// Whether the match runs: a client holds its entity unpredicted, as the server sends it.
    fn running(ended: Query<'_, '_, (), (With<MatchEnd>, Allow<Unpredicted>)>) -> bool {
        ended.is_empty()
    }
}

impl SimComponent for MatchEnd {
    const NAME: &'static str = "mode.match_end";
    type Host = MatchEntity;

    // A winner that is not one of the mode's teams has no name to report.
    fn check(&self, world: &World, _: Entity) -> bool {
        match self.result {
            MatchResult::Won(team) => world
                .get_non_send::<View>()
                .is_none_or(|view| view.has_team(team)),
            MatchResult::Draw => true,
        }
    }
}

impl Replication for MatchEnd {
    const KIND: DataKind = DataKind::Match;
    type Sending = SentOnce;
}
