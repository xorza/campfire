use std::rc::Rc;

use campfire_script::rhai::{Array, Dynamic, ImmutableString};

use crate::mode::marker::Marker;

/// `ctx.map`: the map's paths, by name, and its markers, in the map's order.
#[derive(Debug, Clone, Default)]
pub(crate) struct GameMap {
    pub(crate) paths: Array,
    markers: Rc<[Marker]>,
}

impl GameMap {
    /// The map of `paths`, by name, and `markers`.
    pub(crate) fn new(
        paths: impl Iterator<Item = ImmutableString>,
        markers: impl Iterator<Item = Marker>,
    ) -> GameMap {
        GameMap {
            paths: paths.map(Dynamic::from).collect(),
            markers: markers.collect(),
        }
    }

    /// The markers with tag `tag`, in the map's order.
    pub(crate) fn markers(&self, tag: &str) -> Array {
        self.markers
            .iter()
            .filter(|marker| marker.has(tag))
            .map(|marker| Dynamic::from(marker.clone()))
            .collect()
    }
}
