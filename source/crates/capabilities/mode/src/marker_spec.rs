use std::collections::BTreeMap;

use campfire_geometry::Bounds;
use campfire_math::Vec3;
use campfire_values::{DeclaredName, Team};

use crate::mode_data::ModeParam;

/// A marker of the map, names resolved: its name, its tags, its point, its region and its team
/// if it names them, and its params.
#[derive(Debug, Clone)]
pub struct MarkerSpec {
    pub name: DeclaredName,
    pub tags: Box<[DeclaredName]>,
    pub pos: Option<Vec3>,
    pub region: Option<Bounds>,
    pub team: Option<Team>,
    pub params: BTreeMap<DeclaredName, ModeParam>,
}
