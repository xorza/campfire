use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::{SimResource, StableId};
use serde::{Deserialize, Serialize};

use crate::progression::track_book::TrackBook;
use crate::stats::level::Level;
use crate::units::track_id::TrackId;

/// The levels units reached, in the order reached, whose `on_level_up` has yet to run. The Mode
/// stage runs the calls from the front; the level-ups whose call found the mode pool spent stay,
/// and run first in a later tick's Mode stage.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LevelUps(pub(crate) Vec<LevelUp>);

/// A level a unit reached on a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LevelUp {
    pub(crate) unit: StableId,
    pub(crate) track: TrackId,
    pub(crate) level: Level,
}

impl SimResource for LevelUps {
    const NAME: &'static str = "progression.level_ups";

    // A track the mode lacks has no name for `on_level_up` to read.
    fn check(&self, world: &World) -> bool {
        let book = world.get_resource::<TrackBook>();
        self.0
            .iter()
            .all(|level_up| book.is_some_and(|book| book.has(level_up.track)))
    }
}
