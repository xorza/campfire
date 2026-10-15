use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_geometry::{BodyBox, Radius, Shape};
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentOnce};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::layer::Layer;
use crate::unit_entity::UnitEntity;

/// A unit's body on the ground plane, on its layer: a circle of a radius, or a box, which a unit
/// that does not walk on a planar map may have. Living bodies of one layer do not overlap, and
/// every range counts from a body's edge; a unit with no body is a point, on the first layer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[component(immutable)]
pub struct Body {
    shape: Shape,
    layer: Layer,
}

impl Body {
    /// A circle on the first layer; `None` unless `radius` is positive and at most
    /// `MAX_BOUND`.
    pub const fn new(radius: Num) -> Option<Body> {
        if radius.to_bits() <= 0 {
            return None;
        }
        let Some(radius) = Radius::new(radius) else {
            return None;
        };
        Some(Body {
            shape: Shape::Circle(radius),
            layer: Layer::FIRST,
        })
    }

    /// The body of the built box `body`, on the first layer.
    pub const fn of_box(body: BodyBox) -> Body {
        Body {
            shape: Shape::Box(body),
            layer: Layer::FIRST,
        }
    }

    /// The body on `layer`.
    #[must_use]
    pub(crate) const fn on(self, layer: Layer) -> Body {
        Body { layer, ..self }
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
    pub const fn shape_of(body: Option<&Body>) -> Shape {
        match body {
            Some(body) => body.shape,
            None => Shape::POINT,
        }
    }

    /// The layer of a unit with `body`, or the first for a unit with none.
    pub const fn layer_of(body: Option<&Body>) -> Layer {
        match body {
            Some(body) => body.layer,
            None => Layer::FIRST,
        }
    }
}

impl SimComponent for Body {
    const NAME: &'static str = "units.body";
    type Host = UnitEntity;

    // Its decode bounds its radius or its box; a layer only compares with another.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Body {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}

/// A snapshot is untrusted, so a radius `new` refuses fails to decode, and a box its own decode
/// refuses.
impl<'de> Deserialize<'de> for Body {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Body, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            shape: Shape,
            layer: Layer,
        }
        let fields = Fields::deserialize(deserializer)?;
        let body = match fields.shape {
            Shape::Circle(radius) => Body::new(radius.get())
                .ok_or_else(|| D::Error::custom("a body radius not positive or beyond 2,048 m"))?,
            Shape::Box(body) => Body::of_box(body),
        };
        Ok(body.on(fields.layer))
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::cmp::Ordering;

    use campfire_geometry::{Metric, Placed, Radius, Shape};
    use campfire_math::{Num, Vec3};

    use crate::body::Body;

    impl Body {
        /// Whether `range` from the edge of `from_body` at `from` reaches the edge of `to_body`
        /// at `to`, on a planar map, by the reach rule; a unit with no body is a point.
        pub fn reaches(
            from: Vec3,
            from_body: Option<&Body>,
            range: Num,
            to: Vec3,
            to_body: Option<&Body>,
        ) -> bool {
            let from = Placed::new(from, Body::shape_of(from_body));
            Metric::Planar.reaches(from, range, Placed::new(to, Body::shape_of(to_body)))
        }

        /// Whether `range` from the edge of `from_body` at `from` reaches the least circle round
        /// `to` that holds `to_body`, as a body measured by its bound would be reached.
        pub fn reaches_bound(
            from: Vec3,
            from_body: Option<&Body>,
            range: Num,
            to: Vec3,
            to_body: Option<&Body>,
        ) -> bool {
            let bound = Radius::new(Body::shape_of(to_body).bound()).expect("a bound is a radius");
            let bound = Placed::new(to, Shape::Circle(bound));
            Metric::Planar.reaches(Placed::new(from, Body::shape_of(from_body)), range, bound)
        }

        /// Whether this circle at `pos` overlaps the box `body` at `center`: a body collision
        /// left inside the box.
        pub fn sinks_into(&self, pos: Vec3, body: &Body, center: Vec3) -> bool {
            let (Shape::Circle(radius), Shape::Box(boxed)) = (self.shape(), body.shape()) else {
                panic!("a circle and a box");
            };
            boxed.nearest(center, pos, radius.get()) == Ordering::Less
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Binary;

    use super::*;
    use crate::body_form::BodyForm;
    use crate::facing::Facing;

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
}
