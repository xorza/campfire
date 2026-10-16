use campfire_declare::PerType;
use campfire_math::Num;
use campfire_sim::Capability;
use campfire_units::{Body, BodyForm, Layer};

/// A kind of unit that walks, as routes see it: the layer it moves on and its body's radius, 0
/// for one with no body. The pathing grid keeps the cells each kind may stand in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Walker {
    pub layer: Layer,
    pub radius: Num,
}

impl Walker {
    /// The walker of a unit with `body`, a circle or none; `None` for a box, which never walks.
    pub const fn of(body: Option<&Body>) -> Option<Self> {
        let radius = match body {
            Some(body) => match body.radius() {
                Some(radius) => radius,
                None => return None,
            },
            None => Num::ZERO,
        };
        Some(Self {
            layer: Body::layer_of(body),
            radius,
        })
    }

    /// The walker of a unit that walks, with `body`: a circle or none, as a box never walks,
    /// which `MoveStep`'s check holds of a snapshot too.
    pub(crate) const fn walking(body: Option<&Body>) -> Self {
        Self::of(body).expect("a unit that walks has a circle for a body, or none")
    }

    /// The walker of a unit type of `form`, a circle or none, as its units' bodies make them;
    /// `None` for a box. A circle's spawn angle turns nothing.
    pub fn of_form(form: Option<BodyForm>) -> Option<Self> {
        Self::of(form.map(|form| form.at(Num::ZERO)).as_ref())
    }
}

impl PerType for Walker {
    const OWNER: Option<Capability> = Some(Capability::Navigation);
    const SCRIPTS_ONLY: bool = false;
}
