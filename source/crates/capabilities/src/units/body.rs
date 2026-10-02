use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::units::layer::Layer;

/// A unit's body: a circle of `radius` meters on the ground plane, on its layer. Living bodies of
/// one layer do not overlap, and every range counts from a body's edge; a unit with no body is a
/// point, on the first layer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Body {
    radius: Num,
    layer: Layer,
}

impl Body {
    /// The widest body: wider than any structure a map stands, and small enough that two bodies'
    /// radii and a range add up within a `Num`.
    pub const MAX_RADIUS: Num = Num::from_bits(64 << Num::FRAC_BITS);

    /// A body on the first layer; `None` unless `radius` is positive and at most `MAX_RADIUS`.
    pub const fn new(radius: Num) -> Option<Body> {
        if radius.to_bits() <= 0 || radius.to_bits() > Body::MAX_RADIUS.to_bits() {
            return None;
        }
        Some(Body {
            radius,
            layer: Layer::FIRST,
        })
    }

    /// The body on `layer`.
    #[must_use]
    pub(crate) const fn on(self, layer: Layer) -> Body {
        Body { layer, ..self }
    }

    pub const fn radius(self) -> Num {
        self.radius
    }

    pub(crate) const fn layer(self) -> Layer {
        self.layer
    }

    /// The radius of a unit with `body`, or 0 for a unit with none.
    pub const fn radius_of(body: Option<&Body>) -> Num {
        match body {
            Some(body) => body.radius,
            None => Num::ZERO,
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

    // Its decode bounds its radius; a layer only compares with another.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

/// A snapshot is untrusted, so a radius `new` refuses fails to decode.
impl<'de> Deserialize<'de> for Body {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Body, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            radius: Num,
            layer: Layer,
        }
        let fields = Fields::deserialize(deserializer)?;
        let body = Body::new(fields.radius)
            .ok_or_else(|| D::Error::custom("a body radius not positive or beyond 64 m"))?;
        Ok(body.on(fields.layer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_a_positive_radius_up_to_64_m_on_a_layer() {
        let num = |value| Num::from_int(value).unwrap();
        assert_eq!(Body::new(num(64)).map(Body::radius), Some(num(64)));
        assert_eq!(
            Body::new(Num::EPSILON).map(Body::radius),
            Some(Num::EPSILON)
        );
        for radius in [Num::ZERO, num(-1), num(64) + Num::EPSILON] {
            assert_eq!(Body::new(radius), None, "{radius:?}");
        }
        assert_eq!(Body::radius_of(None), Num::ZERO);
        assert_eq!(Body::radius_of(Body::new(num(2)).as_ref()), num(2));
        // On the first layer, unless placed on another; a unit with no body is on the first.
        let air = Body::new(num(2)).unwrap().on(Layer::new(1));
        assert_eq!(Body::layer_of(Body::new(num(2)).as_ref()), Layer::FIRST);
        assert_eq!(Body::layer_of(Some(&air)), Layer::new(1));
        assert_eq!(Body::layer_of(None), Layer::FIRST);
        let decoded = postcard::from_bytes::<Body>(&postcard::to_allocvec(&air).unwrap());
        assert_eq!(decoded.ok(), Some(air));
    }
}
