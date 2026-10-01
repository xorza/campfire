use std::rc::Rc;

use campfire_script::rhai::{Array, Dynamic, ImmutableString};
use campfire_sim::Position;

use crate::mode::marker::Marker;
use crate::units::team::Team;

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

    /// The point of the first marker with tag `tag` and team `team`, if it has one.
    pub(crate) fn point(&self, tag: &str, team: Team) -> Option<Position> {
        self.markers
            .iter()
            .find(|marker| marker.has(tag) && marker.info().team == Some(team))
            .and_then(|marker| marker.info().pos)
    }
}
