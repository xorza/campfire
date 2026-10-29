use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// An object's geometry as the game reads it (`GeometryInfo`): its shape, its radii and its
/// height, and whether it is small, which the pathfinder routes over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Geometry {
    pub(crate) shape: GeometryShape,
    /// Its radius, or a box's half extent along the object's `x`.
    pub(crate) major: f32,
    /// A box's half extent along the object's `y`.
    pub(crate) minor: f32,
    pub(crate) height: f32,
    pub(crate) small: bool,
}

/// The shapes of `GeometryNames`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeometryShape {
    Sphere,
    Cylinder,
    Box,
}

impl Geometry {
    /// A thing's geometry until its fields say otherwise: a sphere of radius 1, not small.
    pub(super) const DEFAULT: Self = Self {
        shape: GeometryShape::Sphere,
        major: 1.0,
        minor: 1.0,
        height: 1.0,
        small: false,
    };

    /// Its height, as `GeometryInfo::set` takes it: a sphere's its radius, whatever its
    /// `GeometryHeight`.
    pub(crate) const fn height(&self) -> f32 {
        match self.shape {
            GeometryShape::Sphere => self.major,
            GeometryShape::Cylinder | GeometryShape::Box => self.height,
        }
    }

    /// The radius of the circle round it on the ground, as `calcBoundingStuff` computes it: a
    /// box's `√(major² + minor²)`, each step in the game's `f32`, else its major radius.
    #[expect(
        clippy::imprecise_flops,
        reason = "the game's own formula, rounded at each step as the game rounds it"
    )]
    pub(crate) fn bounding_circle(&self) -> f32 {
        match self.shape {
            GeometryShape::Sphere | GeometryShape::Cylinder => self.major,
            GeometryShape::Box => (self.major * self.major + self.minor * self.minor).sqrt(),
        }
    }

    /// The radius of the sphere round it, as `calcBoundingStuff` computes it: a box's
    /// `√(major² + minor² + (height / 2)²)`, a cylinder's larger of half its height and its
    /// radius, a sphere's radius; each step in the game's `f32`.
    pub(crate) fn bounding_sphere(&self) -> f32 {
        let half = self.height * 0.5;
        match self.shape {
            GeometryShape::Sphere => self.major,
            GeometryShape::Cylinder => half.max(self.major),
            GeometryShape::Box => {
                (self.major * self.major + self.minor * self.minor + half * half).sqrt()
            }
        }
    }

    /// Takes a geometry field of `word`; whether `word` is one.
    pub(super) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<bool, IniError> {
        match word {
            "geometry" => {
                let value = at.value()?.to_ascii_lowercase();
                self.shape = match value.as_str() {
                    "sphere" => GeometryShape::Sphere,
                    "cylinder" => GeometryShape::Cylinder,
                    "box" => GeometryShape::Box,
                    _ => return Err(IniError::NotListed { line: at.number }),
                };
            }
            "geometrymajorradius" => self.major = at.real()?,
            "geometryminorradius" => self.minor = at.real()?,
            "geometryheight" => self.height = at.real()?,
            "geometryissmall" => self.small = at.yes()?,
            _ => return Ok(false),
        }
        Ok(true)
    }
}
