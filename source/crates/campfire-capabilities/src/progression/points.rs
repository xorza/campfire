use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::progression::experience::Experience;
use crate::progression::track_book::TrackBook;
use crate::progression::track_set::TrackSet;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentPredicted;
use crate::units::track_id::TrackId;

/// The points a unit with the `level` track has to spend on ranks: one for each level it has
/// there, less those it spent. State, not derived from the level, as `ctx.learn` spends none.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Points(u32);

impl Points {
    /// The points of a unit that spawns with `tracks`, at level 1: one, when they hold
    /// `level_track`, the mode's `level` track.
    pub(crate) fn at_spawn(tracks: TrackSet, level_track: Option<TrackId>) -> Option<Points> {
        level_track
            .is_some_and(|track| tracks.contains(track))
            .then_some(Points(1))
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// Adds a point for each of `levels`, levels reached on the `level` track.
    pub(crate) const fn gain(&mut self, levels: u32) {
        self.0 = self
            .0
            .checked_add(levels)
            .expect("a unit has no more points than levels");
    }

    /// Spends one of its points, which it has.
    pub(crate) const fn spend(&mut self) {
        self.0 = self.0.checked_sub(1).expect("a unit spends a point it has");
    }
}

impl SimComponent for Points {
    const NAME: &'static str = "progression.points";

    // Points come from the levels of the `level` track, so only a unit that has it holds them.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let level_track = world
            .get_resource::<TrackBook>()
            .and_then(TrackBook::level_track);
        let experience = world.get::<Experience>(entity);
        level_track
            .zip(experience)
            .is_some_and(|(track, experience)| experience.get(track).is_some())
    }
}

impl Replication for Points {
    const KIND: DataKind = DataKind::Progression;
    type Sending = SentPredicted;
}
