use bevy_ecs::component::Component;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A unit's body: a circle of `radius` meters on the ground plane. Living bodies do not overlap,
/// and every range counts from a body's edge; a unit with no body is a point.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Body(Num);

impl Body {
    /// The widest body: wider than any structure a map stands, and small enough that two bodies'
    /// radii and a range add up within a `Num`.
    pub const MAX_RADIUS: Num = Num::from_bits(64 << Num::FRAC_BITS);

    /// `None` unless `radius` is positive and at most `MAX_RADIUS`.
    pub const fn new(radius: Num) -> Option<Body> {
        if radius.to_bits() <= 0 || radius.to_bits() > Body::MAX_RADIUS.to_bits() {
            return None;
        }
        Some(Body(radius))
    }

    pub const fn radius(self) -> Num {
        self.0
    }

    /// The radius of a unit with `body`, or 0 for a unit with none.
    pub const fn radius_of(body: Option<&Body>) -> Num {
        match body {
            Some(body) => body.0,
            None => Num::ZERO,
        }
    }
}

impl SimComponent for Body {
    const NAME: &'static str = "units.body";
}

/// A snapshot is untrusted, so a radius `new` refuses fails to decode.
impl<'de> Deserialize<'de> for Body {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Body, D::Error> {
        Body::new(Num::deserialize(deserializer)?)
            .ok_or_else(|| D::Error::custom("a body radius not positive or beyond 64 m"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_a_positive_radius_up_to_64_m() {
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
    }
}
