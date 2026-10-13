use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_core::{PendingCalls, TrackId};
use campfire_sim::{SimResource, StableId};
use campfire_stats::Level;
use serde::{Deserialize, Serialize};

use crate::track_book::TrackBook;

/// The levels units reached, in the order reached, whose `on_level_up` has yet to run. The Mode
/// stage runs the calls from the front; the level-ups whose call found the mode pool spent stay,
/// and run first in a later tick's Mode stage.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelUps(pub PendingCalls<LevelUp>);

/// A level a unit reached on a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelUp {
    pub id: StableId,
    pub track: TrackId,
    pub level: Level,
}

impl SimResource for LevelUps {
    const NAME: &'static str = "progression.level_ups";

    // A track the mode lacks has no name for `on_level_up` to read.
    fn check(&self, world: &World) -> bool {
        let book = world.resource::<TrackBook>();
        self.0
            .iter()
            .all(|level_up| book.names().has(level_up.track))
    }
}
