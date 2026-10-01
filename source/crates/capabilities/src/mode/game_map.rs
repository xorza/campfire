use campfire_script::rhai::{Array, Dynamic, ImmutableString};
use campfire_sim::Position;

/// `ctx.map`: the map's paths, by name, and its neutral spawns.
#[derive(Debug, Clone, Default)]
pub(crate) struct GameMap {
    pub(crate) paths: Array,
    pub(crate) neutral_spawns: Array,
}

/// A neutral spawn of the map, as `ctx.map.neutral_spawns` lists it.
#[derive(Debug, Clone)]
pub(crate) struct NeutralSpawn {
    pub(crate) unit_type: ImmutableString,
    pub(crate) pos: Position,
}

impl GameMap {
    /// The map of `paths`, by name, and `neutral_spawns`.
    pub(crate) fn new(
        paths: impl Iterator<Item = ImmutableString>,
        neutral_spawns: impl Iterator<Item = NeutralSpawn>,
    ) -> GameMap {
        GameMap {
            paths: paths.map(Dynamic::from).collect(),
            neutral_spawns: neutral_spawns.map(Dynamic::from).collect(),
        }
    }
}
