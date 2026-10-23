use campfire_geometry::BodyBox;
use campfire_math::Num;

use crate::body::Body;
use crate::layer::Layer;

/// A unit type's body shape, on no layer yet: a circle, or a box of a size that a spawn's angle
/// turns, of its height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyShape {
    form: Form,
    height: Num,
}

/// A circle's radius, or a box's size, `[width, depth]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Circle(Num),
    Box([Num; 2]),
}

impl BodyShape {
    /// A circle of `radius`, as `Body::new` takes it.
    pub fn circle(radius: Num) -> Option<Self> {
        Body::new(radius).map(|_| Self {
            form: Form::Circle(radius),
            height: Num::ZERO,
        })
    }

    /// A box of `size`, `[width, depth]` in meters, when `BodyBox` takes that size: at every
    /// angle, as the size alone decides.
    pub fn box_sized(size: [Num; 2]) -> Option<Self> {
        BodyBox::new(size, Num::ZERO).map(|_| Self {
            form: Form::Box(size),
            height: Num::ZERO,
        })
    }

    /// The shape `height` meters tall, when `Body::raised` takes that height.
    pub fn raised(self, height: Num) -> Option<Self> {
        Body::holds_height(height).then_some(Self { height, ..self })
    }

    pub const fn is_box(self) -> bool {
        matches!(self.form, Form::Box(_))
    }

    /// The size of a box, `[width, depth]`; `None` for a circle.
    pub const fn box_size(self) -> Option<[Num; 2]> {
        match self.form {
            Form::Box(size) => Some(size),
            Form::Circle(_) => None,
        }
    }

    pub const fn height(self) -> Num {
        self.height
    }

    /// The radius of a circle, as a walker sees it; `None` for a box, which never walks.
    pub const fn circle_radius(self) -> Option<Num> {
        match self.form {
            Form::Circle(radius) => Some(radius),
            Form::Box(_) => None,
        }
    }

    /// The body of the shape on `layer`, turned by `angle` degrees: a box's turn, which a circle
    /// ignores.
    pub(crate) fn body(self, layer: Layer, angle: Num) -> Body {
        let body = match self.form {
            Form::Circle(radius) => {
                Body::new(radius).expect("a shape's radius is one a body takes")
            }
            Form::Box(size) => {
                let body = BodyBox::new(size, angle);
                Body::of_box(body.expect("a shape's size is one a box takes at every angle"))
            }
        };
        body.on(layer)
            .raised(self.height)
            .expect("a shape's height is one a body takes")
    }
}
