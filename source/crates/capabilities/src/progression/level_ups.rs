use bevy_ecs::resource::Resource;
use campfire_sim::{SimResource, StableId};
use serde::{Deserialize, Serialize};

use crate::progression::track_id::TrackId;
use crate::stats::level::Level;

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
}
