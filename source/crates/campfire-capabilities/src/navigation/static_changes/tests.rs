use campfire_math::{Num, Vec3};
use campfire_sim::IdAllocator;

use super::*;
use crate::geometry::shape::Shape;
use crate::units::layer::Layer;

fn at(x: i64, z: i64) -> Position {
    let num = |value: i64| Num::from_int(value).unwrap();
    Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap()
}

#[test]
fn a_route_is_blocked_only_by_a_body_of_its_layer_put_in_since_the_last_check() {
    // A walker of 1 m from (0, 0) by (10, 0) to (10, 10). A post of 1 m at (5, 3) comes 3 m
    // from the first leg and 5 m from the second, past the two radii, 2 m; one at (12, 5)
    // comes 2 m from the second, not closer; one at (11, 5), 1 m. One on the second layer at
    // (5, 0) stands on the first leg, and blocks no walker of the first.
    let mut ids = IdAllocator::default();
    let mut post = |x, z, layer| IndexedBody {
        id: ids.allocate(),
        at: at(x, z),
        shape: Shape::Circle(Num::ONE),
        layer,
    };
    let (far, touching, near, above) = (
        post(5, 3, Layer::FIRST),
        post(12, 5, Layer::FIRST),
        post(11, 5, Layer::FIRST),
        post(5, 0, Layer::new(1)),
    );
    let walker = Walker {
        layer: Layer::FIRST,
        radius: Num::ONE,
    };
    let route = [at(10, 0), at(10, 10)];
    let mut index = BodyIndex::new(Num::ONE);
    let mut changes = StaticChanges::default();
    // Two updates before a check, the second taking a post away: both count.
    index.update(&[far, touching, above]);
    changes.note(&index);
    index.update(&[far, touching]);
    changes.note(&index);
    assert!(changes.removed());
    assert!(!changes.blocks_route(at(0, 0), &route, walker));
    index.update(&[far, touching, near]);
    changes.note(&index);
    assert!(changes.blocks_route(at(0, 0), &route, walker));
    changes.clear();
    assert!(!changes.removed());
    assert!(!changes.blocks_route(at(0, 0), &route, walker));
}
