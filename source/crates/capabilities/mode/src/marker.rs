use std::rc::Rc;

use campfire_math::Vec3;
use campfire_script::{ScriptMap, ScriptText};
use campfire_values::Team;

/// A marker of the map, as `ctx.map.markers(tag)` lists it: a handle on its names resolved.
#[derive(Debug, Clone)]
pub struct Marker(Rc<MarkerInfo>);

/// What a marker holds: its name, its point and its team if it names them, and its params as
/// scripts read them.
#[derive(Debug)]
pub struct MarkerInfo {
    pub name: ScriptText,
    pub pos: Option<Vec3>,
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
}
