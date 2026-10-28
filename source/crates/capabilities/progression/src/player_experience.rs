use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_players::Player;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnChange};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::experience::Experience;
use crate::track_book::TrackBook;
use crate::track_data::TrackHolder;

/// A player's experience and level on each of the mode's player tracks, as a unit's `Experience`
/// holds a unit's (generals.md, A track of players). State, sent to its owner.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerExperience(pub(crate) Experience);

impl PlayerExperience {
    pub const fn get(&self) -> &Experience {
        &self.0
    }
}

impl SimComponent for PlayerExperience {
    const NAME: &'static str = "progression.player_experience";
    type Host = Player;

    // Its tracks are the mode's player tracks, each at a level the track has.
    fn check(&self, world: &World, _: Entity) -> bool {
        let book = world.resource::<TrackBook>();
        self.0.iter().all(|held| {
            book.names().has(held.track)
                && book.holder(held.track) == TrackHolder::Player
                && usize::try_from(held.level.get())
                    .is_ok_and(|level| level <= book.levels(held.track))
        })
    }
}

impl Replication for PlayerExperience {
    const KIND: DataKind = DataKind::Progression;
    type Sending = SentOnChange;
}
