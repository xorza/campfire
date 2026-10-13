use std::sync::Arc;

use campfire_sim::Position;

use crate::geometry::body_box::BodyBox;
use crate::geometry::bounds::Bounds;
use crate::navigation::wall::Wall;
use crate::units::layer::Layer;

/// The map's walls, as a placement tests a box against them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Walls(Arc<[Wall]>);

impl Walls {
    pub(crate) fn new(walls: &[Wall]) -> Walls {
        Walls(walls.into())
    }

    pub(crate) fn walls(&self) -> &[Wall] {
        &self.0
    }

    /// Whether `other` shares these walls, by their one allocation: a copy of a match's walls is
    /// them, with no compare of their points.
    pub(crate) fn same(&self, other: &Walls) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Whether `body`, a box on `layer` at `pos`, has room there among `walls`, none in a match
    /// with none: it lies within `bounds`, and its inside shares no point with a wall of its
    /// layer. The bodies that stand there are the caller's to test.
    pub(crate) fn room_for(
        walls: Option<&Walls>,
        bounds: Bounds,
        pos: Position,
        body: &BodyBox,
        layer: Layer,
    ) -> bool {
        let (min, max) = (bounds.min(), bounds.max());
        let extent = body.extent();
        let ground = [pos.get().x, pos.get().z];
        let within = (0..2).all(|axis| {
            ground[axis]
                .checked_sub(extent[axis])
                .is_some_and(|low| low >= min[axis])
                && ground[axis]
                    .checked_add(extent[axis])
                    .is_some_and(|high| high <= max[axis])
        });
        let walls = walls.map_or(&[][..], |walls| &walls.0);
        within
            && !walls
                .iter()
                .any(|wall| wall.layer == layer && body.overlaps_polygon(pos, &wall.area))
    }
}
