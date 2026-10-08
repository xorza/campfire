use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::geometry::body_box::BodyBox;
use crate::geometry::shape::Shape;
use crate::units::layer::Layer;

/// A unit's body on the ground plane, on its layer: a circle of a radius, or a box, which a unit
/// that does not walk on a planar map may have. Living bodies of one layer do not overlap, and
/// every range counts from a body's edge; a unit with no body is a point, on the first layer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Body {
    shape: Shape,
    layer: Layer,
}

/// A unit type's body before a unit of it spawns: a circle, or a box of a size that the spawn's
/// angle turns, on its layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyForm {
    form: Form,
    layer: Layer,
}

/// The shape of a `BodyForm`: a circle's radius, or a box's size, `[width, height]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Circle(Num),
    Box([Num; 2]),
}

impl Body {
    /// The widest body: wider than any structure a map stands, and small enough that two bodies'
    /// radii and a range add up within a `Num`.
    pub const MAX_RADIUS: Num = Num::from_bits(64 << Num::FRAC_BITS);

    /// A circle on the first layer; `None` unless `radius` is positive and at most `MAX_RADIUS`.
    pub const fn new(radius: Num) -> Option<Body> {
        if radius.to_bits() <= 0 || radius.to_bits() > Body::MAX_RADIUS.to_bits() {
            return None;
        }
        Some(Body {
            shape: Shape::Circle(radius),
            layer: Layer::FIRST,
        })
    }

    /// The box `body` on the first layer.
    pub(crate) const fn boxed(body: BodyBox) -> Body {
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

    pub(crate) const fn shape(self) -> Shape {
        self.shape
    }

    /// A circle's radius; `None` for a box.
    pub const fn radius(self) -> Option<Num> {
        match self.shape {
            Shape::Circle(radius) => Some(radius),
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

    pub(crate) const fn layer(self) -> Layer {
        self.layer
    }

    /// The shape of a unit with `body`, or a point for a unit with none.
    pub(crate) const fn shape_of(body: Option<&Body>) -> Shape {
        match body {
            Some(body) => body.shape,
            None => Shape::POINT,
        }
    }

    /// The layer of a unit with `body`, or the first for a unit with none.
    pub(crate) const fn layer_of(body: Option<&Body>) -> Layer {
        match body {
            Some(body) => body.layer,
            None => Layer::FIRST,
        }
    }
}

impl BodyForm {
    /// A circle of `radius` on the first layer, as `Body::new` takes it.
    pub fn circle(radius: Num) -> Option<BodyForm> {
        Body::new(radius).map(|_| BodyForm {
            form: Form::Circle(radius),
            layer: Layer::FIRST,
        })
    }

    /// A box of `size`, `[width, height]` in meters, on the first layer, when `BodyBox` takes
    /// that size: at every angle, as the size alone decides.
    pub fn boxed(size: [Num; 2]) -> Option<BodyForm> {
        BodyBox::new(size, Num::ZERO).map(|_| BodyForm {
            form: Form::Box(size),
            layer: Layer::FIRST,
        })
    }

    /// The form on `layer`.
    #[must_use]
    pub(crate) const fn on(self, layer: Layer) -> BodyForm {
        BodyForm { layer, ..self }
    }

    pub(crate) const fn layer(self) -> Layer {
        self.layer
    }

    /// Whether it is a box.
    pub const fn is_box(self) -> bool {
        matches!(self.form, Form::Box(_))
    }

    /// The body a unit of it spawns with, turned by `angle` degrees: a box's turn, which a
    /// circle ignores.
    pub fn at(self, angle: Num) -> Body {
        let shape = match self.form {
            Form::Circle(radius) => Shape::Circle(radius),
            Form::Box(size) => {
                let body = BodyBox::new(size, angle);
                Shape::Box(body.expect("a form's size is one a box takes at every angle"))
            }
        };
        Body {
            shape,
            layer: self.layer,
        }
    }

    /// The radius of a circle, as a walker sees it; `None` for a box, which never walks.
    pub(crate) const fn radius(self) -> Option<Num> {
        match self.form {
            Form::Circle(radius) => Some(radius),
            Form::Box(_) => None,
        }
    }
}

impl SimComponent for Body {
    const NAME: &'static str = "units.body";

    // Its decode bounds its radius or its box; a layer only compares with another.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
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
            Shape::Circle(radius) => Body::new(radius)
                .ok_or_else(|| D::Error::custom("a body radius not positive or beyond 64 m"))?,
            Shape::Box(body) => Body::boxed(body),
        };
        Ok(body.on(fields.layer))
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::cmp::Ordering;

    use campfire_math::Num;
    use campfire_sim::Position;

    use crate::geometry::metric::Metric;
    use crate::geometry::shape::Shape;
    use crate::units::body::Body;

    /// Whether `range` from the edge of `from_body` at `from` reaches the edge of `to_body` at
    /// `to`, on a planar map, by the reach rule; a unit with no body is a point.
    pub fn reaches(
        from: Position,
        from_body: Option<&Body>,
        range: Num,
        to: Position,
        to_body: Option<&Body>,
    ) -> bool {
        let (from_shape, to_shape) = (Body::shape_of(from_body), Body::shape_of(to_body));
        Metric::Planar.reaches(from, from_shape, range, to, to_shape)
    }

    /// Whether `range` from the edge of `from_body` at `from` reaches the least circle round `to`
    /// that holds `to_body`, as a body measured by its bound would be reached.
    pub fn reaches_bound(
        from: Position,
        from_body: Option<&Body>,
        range: Num,
        to: Position,
        to_body: Option<&Body>,
    ) -> bool {
        let bound = Shape::Circle(Body::shape_of(to_body).bound());
        Metric::Planar.reaches(from, Body::shape_of(from_body), range, to, bound)
    }

    /// Whether the circle `walker` at `at`, its radius less `slack`, overlaps the box `body` at
    /// `centre`: a body collision left inside the box by more than the rounding of its push.
    pub fn sinks_into(
        at: Position,
        walker: &Body,
        slack: Num,
        centre: Position,
        body: &Body,
    ) -> bool {
        let (Shape::Circle(radius), Shape::Box(boxed)) = (walker.shape(), body.shape()) else {
            panic!("a circle and a box");
        };
        boxed.nearest(centre, at, radius - slack) == Ordering::Less
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_a_circle_up_to_64_m_or_a_box_on_a_layer() {
        assert_eq!(
            Body::new(Num::int(64)).and_then(Body::radius),
            Some(Num::int(64))
        );
        assert_eq!(
            Body::new(Num::EPSILON).and_then(Body::radius),
            Some(Num::EPSILON)
        );
        for radius in [Num::ZERO, Num::int(-1), Num::int(64) + Num::EPSILON] {
            assert_eq!(Body::new(radius), None, "{radius:?}");
            assert_eq!(BodyForm::circle(radius), None, "{radius:?}");
        }
        assert_eq!(Body::shape_of(None), Shape::POINT);
        let circle = Body::new(Num::int(2)).unwrap();
        assert_eq!(Body::shape_of(Some(&circle)), Shape::Circle(Num::int(2)));
        // On the first layer, unless placed on another; a unit with no body is on the first.
        let air = circle.on(Layer::new(1));
        assert_eq!(Body::layer_of(Some(&circle)), Layer::FIRST);
        assert_eq!(Body::layer_of(Some(&air)), Layer::new(1));
        assert_eq!(Body::layer_of(None), Layer::FIRST);
        let decoded = postcard::from_bytes::<Body>(&postcard::to_allocvec(&air).unwrap());
        assert_eq!(decoded.ok(), Some(air));
        // A form spawns its body: a circle as it is, a box of 4 × 2 m turned by the spawn's
        // angle, which keeps the form's layer; the box has half edges and no radius.
        let form = BodyForm::circle(Num::int(2)).unwrap();
        assert_eq!(form.at(Num::int(90)), circle);
        let boxed = BodyForm::boxed([Num::int(4), Num::int(2)])
            .unwrap()
            .on(Layer::new(1));
        let turned = boxed.at(Num::int(90));
        let (one, two) = (Num::ONE, Num::int(2));
        assert_eq!(
            turned.half_edges(),
            Some([[Num::ZERO, two], [-one, Num::ZERO]])
        );
        assert_eq!((turned.radius(), turned.layer()), (None, Layer::new(1)));
        assert!(boxed.is_box() && !form.is_box());
        assert_eq!((boxed.radius(), form.radius()), (None, Some(Num::int(2))));
        let decoded = postcard::from_bytes::<Body>(&postcard::to_allocvec(&turned).unwrap());
        assert_eq!(decoded.ok(), Some(turned));
        // A box of a size `BodyBox` refuses is no form.
        assert_eq!(BodyForm::boxed([Num::int(126), Num::int(1)]), None);
    }
}
