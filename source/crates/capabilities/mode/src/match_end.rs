use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, With};
use bevy_ecs::system::Query;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_players::MatchEntity;
use campfire_script::View;
use campfire_sim::{SimComponent, Unpredicted};
use campfire_state_types::{DataKind, Replication, SentOnce};
use campfire_values::Team;
use serde::{Deserialize, Serialize};

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

/// How a match ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchResult {
    Won(Team),
    Draw,
}

impl MatchEnd {
    pub const fn new(tick: Tick, result: MatchResult) -> Self {
        Self { tick, result }
    }

    pub const fn tick(self) -> Tick {
        self.tick
    }

    pub const fn result(self) -> MatchResult {
        self.result
    }

    /// Whether the match runs, the condition of every stage: a client holds its entity
    /// unpredicted, as the server sends it. The tick still counts on once it ended, so a log may
    /// hold ticks after the end, which change nothing.
    pub(crate) fn running(ended: Query<'_, '_, (), (With<Self>, Allow<Unpredicted>)>) -> bool {
        ended.is_empty()
    }
}

impl SimComponent for MatchEnd {
    const NAME: &'static str = "mode.match_end";
    type Host = MatchEntity;

    // A winner that is not one of the mode's teams has no name to report.
    fn check(&self, world: &World, _: Entity) -> bool {
        match self.result {
            MatchResult::Won(team) => world.non_send::<View>().names().has_team(team),
            MatchResult::Draw => true,
        }
    }
}

impl Replication for MatchEnd {
    const KIND: DataKind = DataKind::Match;
    type Sending = SentOnce;
}
