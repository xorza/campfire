use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_players::Player;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnChange};
use campfire_stats::Level;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::player_experience::PlayerExperience;
use crate::track_book::TrackBook;

/// The points a player has to spend on perks: those the levels of its tracks gave, less the costs
/// of the perks it learned (generals.md, A track of players). State, sent to its owner.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerPoints(u32);

impl PlayerPoints {
    /// The points of a player as the match starts: those level 1 of each of `book`'s player
    /// tracks gives; none in a mode whose player tracks give no point.
    pub fn at_start(book: &TrackBook) -> Option<Self> {
        let tracks = book.player_tracks();
        tracks
            .iter()
            .any(|track| book.gives_points(track))
            .then(|| {
                let points = tracks
                    .iter()
                    .map(|track| book.points_to(track, Level::default()));
                Self(points.sum())
            })
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// Adds `points`, which levels reached on its tracks gave.
    pub const fn gain(&mut self, points: u32) {
        self.0 = self
            .0
            .checked_add(points)
            .expect("a player has no more points than its levels gave");
    }
}

impl PlayerPoints {
    /// Spends `cost`, at most what it has, on a perk.
    pub(crate) const fn spend(&mut self, cost: u32) {
        self.0 = self
            .0
            .checked_sub(cost)
            .expect("a player spends what it has");
    }
}

impl SimComponent for PlayerPoints {
    const NAME: &'static str = "progression.player_points";
    type Host = Player;

    // At most what the levels of its tracks gave.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<TrackBook>();
        let given = world
            .get::<PlayerExperience>(entity)
            .map_or(0, |experience| {
                let tracks = experience.get().iter();
                tracks
                    .map(|held| book.points_to(held.track, held.level))
                    .sum()
            });
        self.0 <= given
    }
}

impl Replication for PlayerPoints {
    const KIND: DataKind = DataKind::Progression;
    type Sending = SentOnChange;
}
