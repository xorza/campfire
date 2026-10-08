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

impl Body {
    /// A circle on the first layer; `None` unless `radius` is positive and at most `Shape::MAX_BOUND`.
    pub const fn new(radius: Num) -> Option<Body> {
        if radius.to_bits() <= 0 || radius.to_bits() > Shape::MAX_BOUND.to_bits() {
            return None;
        }
        Some(Body {
            shape: Shape::Circle(radius),
            layer: Layer::FIRST,
        })
    }

    /// The body of the built box `body`, on the first layer.
    pub(crate) const fn of_box(body: BodyBox) -> Body {
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
            Shape::Box(body) => Body::of_box(body),
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
    use crate::units::body_form::BodyForm;

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
        let boxed = BodyForm::box_sized([Num::int(4), Num::int(2)])
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
        assert_eq!(
            (boxed.circle_radius(), form.circle_radius()),
            (None, Some(Num::int(2)))
        );
        let decoded = postcard::from_bytes::<Body>(&postcard::to_allocvec(&turned).unwrap());
        assert_eq!(decoded.ok(), Some(turned));
        // A box of a size `BodyBox` refuses is no form.
        assert_eq!(BodyForm::box_sized([Num::int(126), Num::int(1)]), None);
    }
}
