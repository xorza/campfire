use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::Res;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{SimResource, SimSet};
use serde::{Deserialize, Serialize};

use crate::units::script_view::View;
use crate::units::team::Team;

/// The end of a match: the tick whose Mode stage ended it, and its result. It is state, so the
/// final state hash a verifier checks holds the result too; once it exists, no stage runs.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchEnd {
    tick: Tick,
    result: MatchResult,
}

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

    pub const fn tick(self) -> Tick {
        self.tick
    }

    pub const fn result(self) -> MatchResult {
        self.result
    }

    /// Stops every stage of `schedule` once a match ended, from the next stage on: each stage
    /// checks for the end as it starts, so a stage that ends the match is the last to run. The
    /// tick still counts on, so a log may hold ticks after the end, which change nothing.
    pub(crate) fn stop_stages(schedule: &mut Schedule) {
        for stage in SimSet::ALL {
            schedule.configure_sets(stage.run_if(MatchEnd::running));
        }
    }

    const fn running(end: Option<Res<'_, MatchEnd>>) -> bool {
        end.is_none()
    }
}

impl SimResource for MatchEnd {
    const NAME: &'static str = "mode.match_end";

    // A winner that is not one of the mode's teams has no name to report.
    fn check(&self, world: &World) -> bool {
        match self.result {
            MatchResult::Won(team) => world
                .get_non_send::<View>()
                .is_none_or(|view| view.has_team(team)),
            MatchResult::Draw => true,
        }
    }
}
