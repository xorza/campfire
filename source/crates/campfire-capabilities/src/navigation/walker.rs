use campfire_math::Num;

use crate::units::body::{Body, BodyForm};
use crate::units::layer::Layer;

/// A kind of unit that walks, as routes see it: the layer it moves on and its body's radius, 0
/// for one with no body. The pathing grid keeps the cells each kind may stand in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Walker {
    pub layer: Layer,
    pub radius: Num,
}

impl Walker {
    /// The walker of a unit with `body`, a circle or none; `None` for a box, which never walks.
    pub const fn of(body: Option<&Body>) -> Option<Walker> {
        let radius = match body {
            Some(body) => match body.radius() {
                Some(radius) => radius,
                None => return None,
            },
            None => Num::ZERO,
        };
        Some(Walker {
            layer: Body::layer_of(body),
            radius,
        })
    }

    /// The walker of a unit that walks, with `body`: a circle or none, as a box never walks,
    /// which `MoveStep`'s check holds of a snapshot too.
    pub fn walking(body: Option<&Body>) -> Walker {
        Walker::of(body).expect("a unit that walks has a circle for a body, or none")
    }

    /// The walker of a unit type of `form`, a circle or none; `None` for a box.
    pub fn of_form(form: Option<BodyForm>) -> Option<Walker> {
        let Some(form) = form else {
            return Some(Walker {
                layer: Layer::FIRST,
                radius: Num::ZERO,
            });
        };
        Some(Walker {
            layer: form.layer(),
            radius: form.radius()?,
        })
    }
}
