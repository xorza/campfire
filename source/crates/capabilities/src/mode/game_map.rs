use std::rc::Rc;

use campfire_script::rhai::{Array, Dynamic, ImmutableString};

use crate::mode::marker::Marker;

/// `ctx.map`: the map's paths, by name, and its markers, in the map's order. A clone shares
/// them, as each `ctx.map` read gives one.
#[derive(Debug, Clone, Default)]
pub(crate) struct GameMap(Rc<MapParts>);

/// The parts of a `GameMap`.
#[derive(Debug, Default)]
struct MapParts {
    paths: Array,
    markers: Box<[Marker]>,
}

impl GameMap {
    /// The map of `paths`, by name, and `markers`.
    pub(crate) fn new(
        paths: impl Iterator<Item = ImmutableString>,
        markers: impl Iterator<Item = Marker>,
    ) -> GameMap {
        GameMap(Rc::new(MapParts {
            paths: paths.map(Dynamic::from).collect(),
            markers: markers.collect(),
        }))
    }

    /// The paths' names, by path.
    pub(crate) fn paths(&self) -> Array {
        self.0.paths.clone()
    }

    /// The markers with tag `tag`, in the map's order.
    pub(crate) fn markers(&self, tag: &str) -> Array {
        self.0
            .markers
            .iter()
            .filter(|marker| marker.has(tag))
            .map(|marker| Dynamic::from(marker.clone()))
            .collect()
    }
}
