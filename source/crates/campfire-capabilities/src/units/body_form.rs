use campfire_math::Num;

use crate::geometry::body_box::BodyBox;
use crate::units::body::Body;
use crate::units::layer::Layer;

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
    pub fn box_sized(size: [Num; 2]) -> Option<BodyForm> {
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
        let body = match self.form {
            Form::Circle(radius) => Body::new(radius).expect("a form's radius is one a body takes"),
            Form::Box(size) => {
                let body = BodyBox::new(size, angle);
                Body::of_box(body.expect("a form's size is one a box takes at every angle"))
            }
        };
        body.on(self.layer)
    }

    /// The radius of a circle form, as a walker sees it; `None` for a box, which never walks.
    pub(crate) const fn circle_radius(self) -> Option<Num> {
        match self.form {
            Form::Circle(radius) => Some(radius),
            Form::Box(_) => None,
        }
    }
}
