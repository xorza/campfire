use std::rc::Rc;

use campfire_script::rhai::{Array, Dynamic, ImmutableString};

use crate::mode::marker::{Marker, MarkerInfo};
use crate::mode::marker_spec::MarkerSpec;

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
    /// The map of `paths`, by name, and `markers`, their params as scripts read them.
    pub(crate) fn new(
        paths: impl Iterator<Item = ImmutableString>,
        markers: &[MarkerSpec],
    ) -> GameMap {
        let markers = markers.iter().map(|marker| {
            let params = marker
                .params
                .iter()
                .map(|(name, param)| (name.as_str().into(), param.to_dynamic()));
            Marker::new(MarkerInfo {
                name: (*marker.name).into(),
                tags: marker.tags.clone(),
                pos: marker.pos,
                team: marker.team,
                params: params.collect(),
            })
        });
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
