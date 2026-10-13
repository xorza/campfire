use std::rc::Rc;

use campfire_sim::Position;

use crate::scripts::script_map::ScriptMap;
use crate::scripts::script_text::ScriptText;
use crate::units::team::Team;
use crate::values::declared_name::DeclaredName;

/// A marker of the map, as `ctx.map.markers(tag)` lists it: a handle on its names resolved.
#[derive(Debug, Clone)]
pub(crate) struct Marker(Rc<MarkerInfo>);

/// What a marker holds: its name, its tags, its point and its team if it names them, and its
/// params as scripts read them.
#[derive(Debug)]
pub(crate) struct MarkerInfo {
    pub(crate) name: ScriptText,
    pub(crate) tags: Box<[DeclaredName]>,
    pub(crate) pos: Option<Position>,
    pub(crate) team: Option<Team>,
    pub(crate) params: ScriptMap,
}

impl Marker {
    pub(crate) fn new(info: MarkerInfo) -> Marker {
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
