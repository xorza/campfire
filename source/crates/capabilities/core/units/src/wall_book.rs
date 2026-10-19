use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_geometry::BodyBox;
use campfire_math::Vec3;
use campfire_script::MatchNames;

use crate::layer::Layer;
use crate::wall::Wall;

/// The map's walls, as a placement tests a box against them and the pathing grid blocks its
/// cells: map data, not state, which the capability that reads the map's walls builds.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct WallBook(Arc<[Wall]>);

impl WallBook {
    pub fn new(walls: &[Wall]) -> Self {
        Self(walls.into())
    }

    pub fn walls(&self) -> &[Wall] {
        &self.0
    }

    /// Whether `other` shares these walls, by their one allocation: a copy of a match's walls is
    /// them, with no compare of their points.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Whether the inside of `body`, a box on `layer` at `position`, shares a point with a wall of
    /// its layer.
    pub fn overlaps_box(&self, position: Vec3, body: &BodyBox, layer: Layer) -> bool {
        self.0
            .iter()
            .any(|wall| wall.layer == layer && body.overlaps_polygon(position, &wall.area))
    }
}

impl Book for WallBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}
