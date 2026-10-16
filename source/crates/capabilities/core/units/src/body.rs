use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_geometry::{BodyBox, MAX_BOUND, Metric, Placed, Radius, Shape, Volume};
use campfire_math::{Num, Vec3};
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentOnce};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::layer::Layer;
use crate::map_book::MapBook;
use crate::unit_entity::UnitEntity;

/// A unit's body on the ground plane, on its layer: a circle of a radius, or a box, which a unit
/// that does not walk on a planar map may have, and its height, which makes it a volume
/// (04-capabilities/00-overview.md, A body has a height). Living bodies of one layer do not
/// overlap, and every range counts from a body's edge; a unit with no body is a point, on the first
/// layer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[component(immutable)]
pub struct Body {
    shape: Shape,
    layer: Layer,
    height: Num,
}

impl Body {
    /// A circle on the first layer; `None` unless `radius` is positive and at most
    /// `MAX_BOUND`.
    pub const fn new(radius: Num) -> Option<Self> {
        if radius.to_bits() <= 0 {
            return None;
        }
        let Some(radius) = Radius::new(radius) else {
            return None;
        };
        Some(Self {
            shape: Shape::Circle(radius),
            layer: Layer::FIRST,
            height: Num::ZERO,
        })
    }

    /// The body of the built box `body`, on the first layer.
    pub const fn of_box(body: BodyBox) -> Self {
        Self {
            shape: Shape::Box(body),
            layer: Layer::FIRST,
            height: Num::ZERO,
        }
    }

    /// The body on `layer`.
    #[must_use]
    pub(crate) const fn on(self, layer: Layer) -> Self {
        Self { layer, ..self }
    }

    /// The body `height` meters tall; `None` unless `height` is at least 0 and at most
    /// `MAX_BOUND`, as every body's bound is.
    pub const fn raised(self, height: Num) -> Option<Self> {
        if !Self::holds_height(height) {
            return None;
        }
        Some(Self { height, ..self })
    }

    /// Whether a body may be `height` meters tall, as `raised` takes it.
    pub(crate) const fn holds_height(height: Num) -> bool {
        height.to_bits() >= 0 && height.to_bits() <= MAX_BOUND.to_bits()
    }

    pub const fn height(self) -> Num {
        self.height
    }

    pub const fn shape(self) -> Shape {
        self.shape
    }

    /// A circle's radius; `None` for a box.
    pub const fn radius(self) -> Option<Num> {
        match self.shape {
            Shape::Circle(radius) => Some(radius.get()),
            Shape::Box(_) => None,
        }
    }

    /// A box's half edges, `[x, z]` each; `None` for a circle.
    pub const fn half_edges(self) -> Option<[[Num; 2]; 2]> {
        match self.shape {
            Shape::Circle(_) => None,
            Shape::Box(body) => Some(body.half_edges()),
        }
    }

    pub const fn layer(self) -> Layer {
        self.layer
    }

    /// The shape of a unit with `body`, or a point for a unit with none.
    pub const fn shape_of(body: Option<&Self>) -> Shape {
        match body {
            Some(body) => body.shape,
            None => Shape::POINT,
        }
    }

    /// The volume of a unit with `body` at `pos`, on the map of `map`: its shape from the
    /// position's point in space up by its height; a unit with no body is a point there.
    pub fn volume(body: Option<&Self>, pos: Vec3, map: &MapBook) -> Volume {
        let base = Placed::new(map.point(pos), Self::shape_of(body));
        Volume::new(base, body.map_or(Num::ZERO, |body| body.height))
    }

    /// The center of a unit with `body` at `pos`, on the map of `map`: the position's point in
    /// space raised by half its height, which a shot aims at.
    pub fn center(body: Option<&Self>, pos: Vec3, map: &MapBook) -> Vec3 {
        Self::volume(body, pos, map).center()
    }

    /// The layer of a unit with `body`, or the first for a unit with none.
    pub const fn layer_of(body: Option<&Self>) -> Layer {
        match body {
            Some(body) => body.layer,
            None => Layer::FIRST,
        }
    }
}

impl SimComponent for Body {
    const NAME: &'static str = "units.body";
    type Host = UnitEntity;

    // Its decode bounds its radius or its box; a layer only compares with another. A box only on
    // a planar map, as the load gives one, which a spatial map's measures do not take.
    fn check(&self, world: &World, _: Entity) -> bool {
        matches!(self.shape, Shape::Circle(_))
            || world.resource::<MapBook>().metric == Metric::Planar
    }
}

impl Replication for Body {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}

/// A snapshot is untrusted, so a radius `new` refuses fails to decode, a box its own decode
/// refuses, and a height `raised` refuses.
impl<'de> Deserialize<'de> for Body {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            shape: Shape,
            layer: Layer,
            height: Num,
        }
        let fields = Fields::deserialize(deserializer)?;
        let body = match fields.shape {
            Shape::Circle(radius) => Self::new(radius.get())
                .ok_or_else(|| D::Error::custom("a body radius not positive or beyond 2,048 m"))?,
            Shape::Box(body) => Self::of_box(body),
        };
        body.on(fields.layer)
            .raised(fields.height)
            .ok_or_else(|| D::Error::custom("a body height below 0 or beyond 2,048 m"))
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_geometry::{Metric, Placed, Radius, Shape};
    use campfire_math::{Num, Vec3};

    use crate::body::Body;

    impl Body {
        /// Whether `range` from the edge of `from_body` at `from` reaches the edge of `to_body`
        /// at `to`, on a planar map, by the reach rule; a unit with no body is a point.
        pub fn reaches(
            from: Vec3,
            from_body: Option<&Self>,
            range: Num,
            to: Vec3,
            to_body: Option<&Self>,
        ) -> bool {
            let from = Placed::new(from, Self::shape_of(from_body));
            Metric::Planar.reaches(from, range, Placed::new(to, Self::shape_of(to_body)))
        }

        /// Whether `range` from the edge of `from_body` at `from` reaches the least circle round
        /// `to` that holds `to_body`, as a body measured by its bound would be reached.
        pub fn reaches_bound(
            from: Vec3,
            from_body: Option<&Self>,
            range: Num,
            to: Vec3,
            to_body: Option<&Self>,
        ) -> bool {
            let bound = Radius::new(Self::shape_of(to_body).bound()).expect("a bound is a radius");
            let bound = Placed::new(to, Shape::Circle(bound));
            Metric::Planar.reaches(Placed::new(from, Self::shape_of(from_body)), range, bound)
        }

        /// Whether this circle at `pos` overlaps the box `body` at `center`: a body collision
        /// left inside the box.
        pub fn sinks_into(&self, pos: Vec3, body: &Self, center: Vec3) -> bool {
            let (Shape::Circle(radius), Shape::Box(boxed)) = (self.shape(), body.shape()) else {
                panic!("a circle and a box");
            };
            boxed.closer(center, pos, radius.get())
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;
    use crate::body_form::BodyForm;
    use crate::facing::Facing;
    use crate::ground::{CellSplit, Ground};
    use crate::height_grid::HeightGrid;

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
            assert_eq!(BodyForm::circle(radius), None, "{radius:?}");
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
        let boxed = BodyForm::box_sized([Num::int(4), Num::int(2)])
            .unwrap()
            .at(Num::ZERO);
        let on = |metric| {
            let mut world = World::new();
            world.insert_resource(MapBook {
                metric,
                ..MapBook::default()
            });
            [circle, boxed].map(|body| body.check(&world, Entity::PLACEHOLDER))
        };
        assert_eq!(on(Metric::Planar), [true, true]);
        assert_eq!(on(Metric::Spatial), [true, false]);
        // A form spawns its body: a circle as it is, a box of 4 × 2 m turned by the spawn's
        // angle, which keeps the form's layer; the box has half edges and no radius.
        let form = BodyForm::circle(Num::int(2)).unwrap();
        assert_eq!(form.at(Num::int(90)), circle);
        let boxed = BodyForm::box_sized([Num::int(4), Num::int(2)])
            .unwrap()
            .on(Layer::new(1));
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
        assert!(boxed.is_box() && !form.is_box());
        assert_eq!(
            (boxed.circle_radius(), form.circle_radius()),
            (None, Some(Num::int(2)))
        );
        let decoded = Binary::decode::<Body>(&Binary::encode(&turned));
        assert_eq!(decoded.ok(), Some(turned));
        // A box of a size `BodyBox` refuses is no form.
        assert_eq!(BodyForm::box_sized([Num::int(4094), Num::int(1)]), None);
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
        let map = |metric| MapBook {
            metric,
            ground: ground.clone(),
            ..MapBook::default()
        };
        let [planar, spatial] = [map(Metric::Planar), map(Metric::Spatial)];
        let tall = Body::new(Num::ONE).unwrap().raised(Num::int(2)).unwrap();
        let pos = Vec3::new(Num::int(5), Num::HALF, Num::int(5));
        let at = |y: Num| Vec3::new(pos.x, y, pos.z);
        // On a planar map, 4 + 0.5 + 2 ÷ 2; with no body, its point, 4.5; on a spatial map the
        // position is its point, 0.5 + 1.
        assert_eq!(Body::center(Some(&tall), pos, &planar), at(Num::HALF * 11));
        assert_eq!(Body::center(None, pos, &planar), at(Num::HALF * 9));
        assert_eq!(Body::center(Some(&tall), pos, &spatial), at(Num::HALF * 3));
        let volume = Body::volume(Some(&tall), pos, &planar);
        assert_eq!(volume.base.pos, at(Num::HALF * 9));
        assert_eq!(volume.height, Num::int(2));
        // A point in space gives its position back; a spatial map's are the same. The ground's
        // one coarse cell is 4 m high, and a spatial map has no ground.
        assert_eq!(planar.position(at(Num::HALF * 9)), pos);
        assert_eq!(spatial.position(pos), pos);
        let ends = [pos, Vec3::new(Num::int(9), Num::ZERO, Num::ONE)];
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
        assert!(Binary::decode::<Body>(&raw(-Num::EPSILON)).is_err());
        let form = BodyForm::circle(Num::ONE)
            .unwrap()
            .raised(Num::int(2))
            .unwrap();
        assert_eq!(form.at(Num::ZERO), tall);
    }
}
