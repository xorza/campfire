use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

use crate::progression::track_id::TrackId;
use crate::stats::level::Level;

/// The levels units reached this tick, in the order reached, whose `on_level_up` the Mode stage
/// runs. The Mode stage drains it, and only scripts, which run before it, fill it: each tick ends
/// with none, so it is no state.
#[derive(Resource, Debug, Default)]
pub(crate) struct LevelUps(pub(crate) Vec<LevelUp>);

/// A level a unit reached on a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LevelUp {
    pub(crate) unit: StableId,
    pub(crate) track: TrackId,
    pub(crate) level: Level,
}
