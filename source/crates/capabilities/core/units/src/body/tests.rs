use campfire_codec::{Binary, BinaryError};

use super::*;
use crate::body_form::BodyForm;
use crate::body_shape::BodyShape;
use crate::facing::Facing;
use crate::ground::{CellSplit, Ground};
use crate::height_grid::HeightGrid;
use crate::map_space::MapSpace;

#[test]
fn a_body_is_a_circle_up_to_2048_m_or_a_box_on_a_layer() {
    assert_eq!(
        Body::new(Num::int(2048)).and_then(Body::radius),
        Some(Num::int(2048))
    );
    assert_eq!(
        Body::new(Num::EPSILON).and_then(Body::radius),
        Some(Num::EPSILON)
    );
    for radius in [Num::ZERO, Num::int(-1), Num::int(2048) + Num::EPSILON] {
        assert_eq!(Body::new(radius), None, "{radius:?}");
        assert_eq!(BodyShape::circle(radius), None, "{radius:?}");
    }
    assert_eq!(Body::shape_of(None), Shape::POINT);
    let circle = Body::new(Num::int(2)).unwrap();
    assert_eq!(
        Body::shape_of(Some(&circle)),
        Shape::Circle(Radius::new(Num::int(2)).unwrap())
    );
    // On the first layer, unless placed on another; a unit with no body is on the first.
    let air = circle.on(Layer::new(1));
    assert_eq!(Body::layer_of(Some(&circle)), Layer::FIRST);
    assert_eq!(Body::layer_of(Some(&air)), Layer::new(1));
    assert_eq!(Body::layer_of(None), Layer::FIRST);
    let decoded = Binary::decode::<Body>(&Binary::encode(&air));
    assert_eq!(decoded.ok(), Some(air));
    // A restore keeps a box only on a planar map, and a circle on either.
    let boxed = BodyForm::new(
        BodyShape::box_sized([Num::int(4), Num::int(2)]).unwrap(),
        Layer::FIRST,
    )
    .at(Num::ZERO);
    let on = |metric| {
        let mut world = World::new();
        world.insert_resource(MapBook {
            space: MapSpace::from(metric),
            ..MapBook::default()
        });
        [circle, boxed].map(|body| body.check(&world, Entity::PLACEHOLDER))
    };
    assert_eq!(on(Metric::Planar), [true, true]);
    assert_eq!(on(Metric::Spatial), [true, false]);
    // A form spawns its body: a circle as it is, a box of 4 × 2 m turned by the spawn's
    // angle, which keeps the form's layer; the box has half edges and no radius.
    let form = BodyForm::new(BodyShape::circle(Num::int(2)).unwrap(), Layer::FIRST);
    assert_eq!(form.at(Num::int(90)), circle);
    let boxed = BodyForm::new(
        BodyShape::box_sized([Num::int(4), Num::int(2)]).unwrap(),
        Layer::new(1),
    );
    let turned = boxed.at(Num::int(90));
    let (one, two) = (Num::ONE, Num::int(2));
    assert_eq!(
        turned.half_edges(),
        Some([[Num::ZERO, -two], [one, Num::ZERO]])
    );
    assert_eq!((turned.radius(), turned.layer()), (None, Layer::new(1)));
    // At any angle the box's first half edge points the way a unit of that angle faces, so
    // the model the client turns by the facing covers the box: at 30° and −135° (225°), the
    // facing along it is the angle, within the rounding of its components to a bit, each a
    // 2⁻²⁵ m of a 2 m edge, which moves its angle by under 2⁻²⁰ degrees.
    for angle in [Num::int(30), Num::int(-135)] {
        let [along, _] = boxed.at(angle).half_edges().unwrap();
        let way = Facing::along(along).unwrap().degrees();
        let gap = (way - Facing::of(angle).degrees()).to_bits().unsigned_abs();
        assert!(gap <= 1 << (Num::FRAC_BITS - 16), "{angle}: {way}");
    }
    let (boxed, form) = (boxed.shape(), form.shape());
    assert!(boxed.is_box() && !form.is_box());
    assert_eq!(
        (boxed.circle_radius(), form.circle_radius()),
        (None, Some(Num::int(2)))
    );
    let decoded = Binary::decode::<Body>(&Binary::encode(&turned));
    assert_eq!(decoded.ok(), Some(turned));
    // A box of a size `BodyBox` refuses is no shape.
    assert_eq!(BodyShape::box_sized([Num::int(4094), Num::int(1)]), None);
}

#[test]
fn a_body_is_a_volume_on_its_ground_and_its_center_lies_halfway_up_it() {
    #[derive(Debug, Serialize)]
    struct Raw {
        shape: Shape,
        layer: Layer,
        height: Num,
    }

    // A ground 4 m up everywhere; a body of radius 1 and height 2 at 0.5 m above it.
    let grid = HeightGrid::new([Num::ZERO; 2], Num::int(10), Num::ONE, 2, vec![4; 4]).unwrap();
    let ground = Ground::new(grid, CellSplit::ZeroTwo, None).unwrap();
    let planar = MapBook {
        space: MapSpace::Planar(ground),
        ..MapBook::default()
    };
    let spatial = MapBook {
        space: MapSpace::Spatial,
        ..MapBook::default()
    };
    let tall = Body::new(Num::ONE).unwrap().raised(Num::int(2)).unwrap();
    let position = Vec3::new(Num::int(5), Num::HALF, Num::int(5));
    let at = |y: Num| Vec3::new(position.x, y, position.z);
    // On a planar map, 4 + 0.5 + 2 ÷ 2; with no body, its point, 4.5; on a spatial map the
    // position is its point, 0.5 + 1.
    assert_eq!(
        Body::center(Some(&tall), position, &planar),
        at(Num::HALF * 11)
    );
    assert_eq!(Body::center(None, position, &planar), at(Num::HALF * 9));
    assert_eq!(
        Body::center(Some(&tall), position, &spatial),
        at(Num::HALF * 3)
    );
    let volume = Body::volume(Some(&tall), position, &planar);
    assert_eq!(volume.base.position, at(Num::HALF * 9));
    assert_eq!(volume.height, Num::int(2));
    // A point in space gives its position back; a spatial map's are the same. The ground's
    // one coarse cell is 4 m high, and a spatial map has no ground.
    assert_eq!(planar.position(at(Num::HALF * 9)), position);
    assert_eq!(spatial.position(position), position);
    let ends = [position, Vec3::new(Num::int(9), Num::ZERO, Num::ONE)];
    assert_eq!(planar.highest_along(ends[0], ends[1]), Some(Num::int(4)));
    assert_eq!(spatial.highest_along(ends[0], ends[1]), None);

    // A height is from 0 to 2,048 m, and a snapshot is untrusted: a decoded height below 0
    // fails, one within the bound holds.
    let body = Body::new(Num::ONE).unwrap();
    assert_eq!(body.raised(MAX_BOUND).map(Body::height), Some(MAX_BOUND));
    assert_eq!(body.raised(-Num::EPSILON), None);
    assert_eq!(body.raised(MAX_BOUND + Num::EPSILON), None);
    let raw = |height| {
        Binary::encode(&Raw {
            shape: body.shape(),
            layer: Layer::FIRST,
            height,
        })
    };
    assert_eq!(Binary::decode::<Body>(&raw(Num::int(2))).ok(), Some(tall));
    assert_eq!(
        Binary::decode::<Body>(&raw(-Num::EPSILON)).err(),
        Some(BinaryError::REFUSED)
    );
    let form = BodyForm::new(
        BodyShape::circle(Num::ONE)
            .unwrap()
            .raised(Num::int(2))
            .unwrap(),
        Layer::FIRST,
    );
    assert_eq!(form.at(Num::ZERO), tall);
}
