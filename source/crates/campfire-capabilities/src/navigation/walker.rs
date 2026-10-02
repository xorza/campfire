use campfire_math::Num;

use crate::units::body::Body;
use crate::units::layer::Layer;

/// A kind of unit that walks, as routes see it: the layer it moves on and its body's radius, 0
/// for one with no body. The pathing grid keeps the cells each kind may stand in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Walker {
    pub layer: Layer,
    pub radius: Num,
}

impl Walker {
    /// The walker of a unit with `body`.
    pub const fn of(body: Option<&Body>) -> Walker {
        Walker {
            layer: Body::layer_of(body),
            radius: Body::radius_of(body),
        }
    }
}
