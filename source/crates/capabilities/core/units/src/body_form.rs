use campfire_declarations::PerId;
use campfire_math::Num;
use campfire_values::UnitType;

use crate::body::Body;
use crate::body_shape::BodyShape;
use crate::layer::Layer;

/// A unit type's body before a unit of it spawns: its shape, on its layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyForm {
    shape: BodyShape,
    layer: Layer,
}

impl BodyForm {
    /// The form of `shape` on `layer`.
    pub const fn new(shape: BodyShape, layer: Layer) -> Self {
        Self { shape, layer }
    }

    pub const fn shape(self) -> BodyShape {
        self.shape
    }

    pub const fn layer(self) -> Layer {
        self.layer
    }

    /// The body a unit of it spawns with, turned by `angle` degrees: a box's turn, which a
    /// circle ignores.
    pub fn at(self, angle: Num) -> Body {
        self.shape.body(self.layer, angle)
    }
}

impl PerId<UnitType> for BodyForm {
    const SCRIPTS_ONLY: bool = false;
}
