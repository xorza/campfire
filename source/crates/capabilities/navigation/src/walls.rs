use std::sync::Arc;

use campfire_geometry::BodyBox;
use campfire_math::Vec3;
use campfire_units::Layer;

use crate::wall::Wall;

/// The map's walls, as a placement tests a box against them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Walls(Arc<[Wall]>);

impl Walls {
    pub fn new(walls: &[Wall]) -> Self {
        Self(walls.into())
    }

    pub fn walls(&self) -> &[Wall] {
        &self.0
    }

    /// Whether `other` shares these walls, by their one allocation: a copy of a match's walls is
    /// them, with no compare of their points.
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Whether the inside of `body`, a box on `layer` at `pos`, shares a point with a wall of its
    /// layer.
    pub fn overlaps_box(&self, pos: Vec3, body: &BodyBox, layer: Layer) -> bool {
        self.0
            .iter()
            .any(|wall| wall.layer == layer && body.overlaps_polygon(pos, &wall.area))
    }
}
