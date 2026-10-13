use std::rc::Rc;

use campfire_core::{ScriptMap, ScriptText, Team};
use campfire_sim::Position;
use campfire_values::DeclaredName;

/// A marker of the map, as `ctx.map.markers(tag)` lists it: a handle on its names resolved.
#[derive(Debug, Clone)]
pub struct Marker(Rc<MarkerInfo>);

/// What a marker holds: its name, its tags, its point and its team if it names them, and its
/// params as scripts read them.
#[derive(Debug)]
pub struct MarkerInfo {
    pub name: ScriptText,
    pub tags: Box<[DeclaredName]>,
    pub pos: Option<Position>,
    pub team: Option<Team>,
    pub(crate) params: ScriptMap,
}

impl Marker {
    pub fn new(info: MarkerInfo) -> Marker {
        Marker(Rc::new(info))
    }

    pub(crate) fn info(&self) -> &MarkerInfo {
        &self.0
    }

    /// Whether it has tag `tag`.
    pub(crate) fn has(&self, tag: &str) -> bool {
        self.0.tags.iter().any(|held| held.as_str() == tag)
    }
}
