use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declarations::Book;
use campfire_geometry::{Bounds, Placed, Polygon};
use campfire_math::Num;
use campfire_names::MatchNames;

use crate::layer::Layer;
use crate::wall::Wall;

/// The map's walls, as a placement tests a body against them and the pathing grid blocks its
/// cells, and the least rectangle of each, which spares a body far from a wall the test of its
/// edges: map data, not state, which the capability that reads the map's walls builds.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct WallBook {
    walls: Arc<[Wall]>,
    bounds: Arc<[Bounds]>,
}

impl WallBook {
    pub fn new(walls: &[Wall]) -> Self {
        Self {
            walls: walls.into(),
            bounds: walls
                .iter()
                .map(|wall| Self::rectangle(&wall.area))
                .collect(),
        }
    }

    /// The least rectangle that holds `area` on the ground plane.
    fn rectangle(area: &Polygon) -> Bounds {
        let points = area.points();
        let fold = |pick: fn(Num, Num) -> Num| {
            points.iter().fold(points[0], |far, point| {
                [0, 1].map(|axis| pick(far[axis], point[axis]))
            })
        };
        Bounds::new(fold(Num::min), fold(Num::max))
            .expect("a polygon whose edges meet only where they follow spans both axes")
    }

    pub fn walls(&self) -> &[Wall] {
        &self.walls
    }

    /// Whether `other` shares these walls, by their one allocation: a copy of a match's walls is
    /// them, with no compare of their points.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.walls, &other.walls)
    }

    /// Whether the inside of `body`, on `layer`, shares a point with a wall of its layer. A wall
    /// whose rectangle the body's meets at most at an edge shares no point with its inside.
    pub fn overlaps(&self, body: Placed, layer: Layer) -> bool {
        let extent = body.shape.extent();
        let ground = [body.position.x, body.position.z];
        let near = |bounds: &Bounds| {
            (0..2).all(|axis| {
                let low = ground[axis].checked_sub(extent[axis]).unwrap_or(Num::MIN);
                let high = ground[axis].checked_add(extent[axis]).unwrap_or(Num::MAX);
                low < bounds.max()[axis] && high > bounds.min()[axis]
            })
        };
        self.walls
            .iter()
            .zip(self.bounds.iter())
            .any(|(wall, bounds)| {
                wall.layer == layer && near(bounds) && body.overlaps_polygon(&wall.area)
            })
    }
}

impl Book for WallBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

#[cfg(test)]
mod tests {
    use campfire_geometry::{Radius, Shape};
    use campfire_math::Vec3;

    use super::*;

    #[test]
    fn a_body_overlaps_a_wall_as_the_walls_polygon_says_its_rectangle_first() {
        // The triangle (0, 0), (4, 0), (0, 4), its rectangle from (0, 0) to (4, 4), and a body of
        // radius 1 at each whole point from (−3, −3) to (6, 6): the rectangle's test passes over
        // the bodies far from it, and the book says what the polygon's own test says, on its
        // layer alone. At (3, 3) a body meets the rectangle and not the triangle.
        let at = |x: i64, z: i64| Vec3::new(Num::int(x), Num::ZERO, Num::int(z));
        let point = |x: i64, z: i64| [Num::int(x), Num::int(z)];
        let area = Polygon::new(vec![point(0, 0), point(4, 0), point(0, 4)]).unwrap();
        let ground = Layer::new(0);
        let book = WallBook::new(&[Wall {
            layer: ground,
            area: area.clone(),
        }]);
        let circle = Shape::Circle(Radius::new(Num::ONE).unwrap());
        for x in -3..=6 {
            for z in -3..=6 {
                let body = Placed::new(at(x, z), circle);
                assert_eq!(
                    book.overlaps(body, ground),
                    body.overlaps_polygon(&area),
                    "({x}, {z})"
                );
                assert!(!book.overlaps(body, Layer::new(1)));
            }
        }
        // Touching the rectangle's edge, at x = 5, is no overlap; within it, at (1, 1), is.
        assert!(!book.overlaps(Placed::new(at(3, 3), circle), ground));
        assert!(!book.overlaps(Placed::new(at(5, 1), circle), ground));
        assert!(book.overlaps(Placed::new(at(1, 1), circle), ground));
    }
}
