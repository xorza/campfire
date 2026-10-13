use std::collections::BTreeMap;

use campfire_core::Team;
use campfire_geometry::Bounds;
use campfire_sim::Position;
use campfire_values::DeclaredName;

use crate::mode_data::ModeParam;

/// A marker of the map, names resolved: its name, its tags, its point, its region and its team
/// if it names them, and its params.
#[derive(Debug, Clone)]
pub struct MarkerSpec {
    pub name: Box<str>,
    pub tags: Box<[DeclaredName]>,
    pub pos: Option<Position>,
    pub region: Option<Bounds>,
    pub team: Option<Team>,
    pub params: BTreeMap<DeclaredName, ModeParam>,
}
