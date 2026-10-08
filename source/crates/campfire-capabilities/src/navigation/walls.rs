use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_sim::Position;

use crate::geometry::body_box::BodyBox;
use crate::geometry::bounds::Bounds;
use crate::navigation::body_index::IndexedBody;
use crate::navigation::wall::Wall;
use crate::units::layer::Layer;

/// The map's walls, as a placement tests a box against them: package data, not state. A restore
/// takes them from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Walls(Arc<[Wall]>);

impl Walls {
    pub(crate) fn new(walls: &[Wall]) -> Walls {
        Walls(walls.into())
    }

    /// Whether `other` shares these walls, by their one allocation: a copy of a match's walls is
    /// them, with no compare of their points.
    pub(crate) fn same(&self, other: &Walls) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Whether `body`, a box on `layer` at `at`, has room there: it lies within `bounds`, and its
    /// inside shares no point with a wall of its layer or with one of `statics`, the bodies that
    /// stand there, each of which the caller found near it on its layer.
    pub(crate) fn room_for(
        &self,
        bounds: Bounds,
        at: Position,
        body: &BodyBox,
        layer: Layer,
        statics: impl IntoIterator<Item = IndexedBody>,
    ) -> bool {
        let (min, max) = (bounds.min(), bounds.max());
        let extent = body.extent();
        let ground = [at.get().x, at.get().z];
        let within = (0..2).all(|axis| {
            ground[axis]
                .checked_sub(extent[axis])
                .is_some_and(|low| low >= min[axis])
                && ground[axis]
                    .checked_add(extent[axis])
                    .is_some_and(|high| high <= max[axis])
        });
        within
            && !self
                .0
                .iter()
                .any(|wall| wall.layer == layer && body.overlaps_polygon(at, &wall.area))
            && statics
                .into_iter()
                .all(|other| !other.overlaps_box(at, body))
    }
}
