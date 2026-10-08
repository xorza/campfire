use std::collections::BTreeMap;

use campfire_sim::Position;

use crate::geometry::bounds::Bounds;
use crate::mode::mode_data::ModeParam;
use crate::units::team::Team;
use crate::values::declared_name::DeclaredName;
use crate::values::name_list::NameList;

/// A marker of the map, names resolved: its name, its tags, its point, its region and its team
/// if it names them, and its params.
#[derive(Debug, Clone)]
pub(crate) struct MarkerSpec {
    pub(crate) name: Box<str>,
    pub(crate) tags: NameList,
    pub(crate) pos: Option<Position>,
    pub(crate) region: Option<Bounds>,
    pub(crate) team: Option<Team>,
    pub(crate) params: BTreeMap<DeclaredName, ModeParam>,
}
