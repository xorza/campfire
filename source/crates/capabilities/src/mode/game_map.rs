use std::rc::Rc;

use crate::mode::marker::{Marker, MarkerInfo};
use crate::mode::marker_spec::MarkerSpec;
use crate::scripts::script_arg::plain_script_arg;
use crate::scripts::script_list::ScriptList;
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;

/// `ctx.map`: the map's paths, by name, and its markers, in the map's order. A clone shares
/// them, as each `ctx.map` read gives one.
#[derive(Debug, Clone, Default)]
pub(crate) struct GameMap(Rc<MapParts>);

/// The parts of a `GameMap`.
#[derive(Debug, Default)]
struct MapParts {
    paths: ScriptList,
    markers: Box<[Marker]>,
}

impl GameMap {
    /// The map of `paths`, by name, and `markers`, their params as scripts read them.
    pub(crate) fn new(paths: impl Iterator<Item = ScriptText>, markers: &[MarkerSpec]) -> GameMap {
        let markers = markers.iter().map(|marker| {
            let params = marker
                .params
                .iter()
                .map(|(name, param)| (name.as_str(), param.to_value()));
            Marker::new(MarkerInfo {
                name: (*marker.name).into(),
                tags: marker.tags.clone(),
                pos: marker.pos,
                team: marker.team,
                params: params.collect(),
            })
        });
        GameMap(Rc::new(MapParts {
            paths: paths.map(ScriptValue::of).collect(),
            markers: markers.collect(),
        }))
    }

    /// The paths' names, by path.
    pub(crate) fn paths(&self) -> ScriptList {
        self.0.paths.clone()
    }

    /// The markers with tag `tag`, in the map's order.
    pub(crate) fn markers(&self, tag: &str) -> ScriptList {
        self.0
            .markers
            .iter()
            .filter(|marker| marker.has(tag))
            .map(|marker| ScriptValue::of(marker.clone()))
            .collect()
    }
}

plain_script_arg!(GameMap);
