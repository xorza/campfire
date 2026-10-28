use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_stats::Level;
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::experience::Experience;
use crate::track_book::TrackBook;
use crate::track_set::TrackSet;

/// The points a unit with the `level` track has to spend on ranks: those its levels there gave,
/// less those it spent. State, not derived from the level, as `ctx.learn` spends none.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Points(u32);

impl Points {
    /// The points of a unit that spawns with `tracks`, at level 1: those level 1 gives, when they
    /// hold `book`'s `level` track and its levels give points; none else.
    pub fn at_spawn(tracks: TrackSet, book: &TrackBook) -> Option<Self> {
        let track = book.level_track().filter(|&track| tracks.contains(track))?;
        book.gives_points(track)
            .then(|| Self(book.points_to(track, Level::default())))
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// Adds `points`, which levels reached on the `level` track gave.
    pub const fn gain(&mut self, points: u32) {
        self.0 = self
            .0
            .checked_add(points)
            .expect("a unit has no more points than its levels gave");
    }

    /// Spends one of its points, which it has.
    pub(crate) const fn spend(&mut self) {
        self.0 = self.0.checked_sub(1).expect("a unit spends a point it has");
    }
}

impl SimComponent for Points {
    const NAME: &'static str = "progression.points";
    type Host = UnitEntity;

    // Points come from the levels of the `level` track, so only a unit that has it holds them,
    // at most those its levels there gave.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<TrackBook>();
        let experience = world.get::<Experience>(entity);
        book.level_track()
            .zip(experience)
            .is_some_and(|(track, experience)| {
                experience
                    .get(track)
                    .is_some_and(|held| self.0 <= book.points_to(track, held.level))
            })
    }
}

impl Replication for Points {
    const KIND: DataKind = DataKind::Progression;
    type Sending = SentPredicted;
}
