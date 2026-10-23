use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, Section, SectionTable, SourceType};
use campfire_math::Num;
use campfire_script::{DataSection, DataTable, UnknownName};
use campfire_values::Named;
use serde::de::Error;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::body_form::BodyForm;
use crate::body_shape::BodyShape;
use crate::layer::Layer;

/// A unit type's `collision` section: its body's shape, a circle of its `radius` or a box of its
/// `box` size, `[width, depth]`, in meters, its `height`, 0 when it names none, and the layer it
/// moves on, the mode's first when it names none, which `bodies` resolves. A type without one has
/// no body: it collides with nothing, ranges count from its center, and it moves on the first
/// layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionData {
    pub shape: BodyShape,
    pub layer: Option<DeclaredName>,
}

/// A radius `Body::new` takes, or a size `BodyBox::new` takes, one of them, and a height
/// `Body::raised` takes, or the section fails to read.
impl<'de> Deserialize<'de> for CollisionData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            radius: Option<Num>,
            #[serde(rename = "box")]
            size: Option<[Num; 2]>,
            #[serde(default)]
            height: Num,
            layer: Option<DeclaredName>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let shape = match (fields.radius, fields.size) {
            (Some(radius), None) => BodyShape::circle(radius)
                .ok_or_else(|| D::Error::custom("a collision radius is positive, up to 2,048 m"))?,
            (None, Some(size)) => BodyShape::box_sized(size).ok_or_else(|| {
                D::Error::custom(
                    "a collision box's sides are at least 2⁻¹⁰ m, its diagonal at most 4,094 m",
                )
            })?,
            _ => {
                return Err(D::Error::custom(
                    "a collision `radius` or `box`, one of them",
                ));
            }
        };
        let shape = shape
            .raised(fields.height)
            .ok_or_else(|| D::Error::custom("a collision height is at least 0, up to 2,048 m"))?;
        Ok(Self {
            shape,
            layer: fields.layer,
        })
    }
}

/// As data writes it, what `Deserialize` reads: its `radius` or its `box`, its `height` unless it
/// is 0, and its `layer` when it names one.
impl Serialize for CollisionData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        match (self.shape.circle_radius(), self.shape.box_size()) {
            (Some(radius), _) => map.serialize_entry("radius", &radius)?,
            (None, size) => {
                map.serialize_entry("box", &size.expect("a form is a circle or a box"))?;
            }
        }
        if self.shape.height() != Num::ZERO {
            map.serialize_entry("height", &self.shape.height())?;
        }
        if let Some(layer) = &self.layer {
            map.serialize_entry("layer", layer)?;
        }
        map.end()
    }
}

impl DataSection for CollisionData {
    const TABLE: DataTable = DataTable::new("A unit type's `collision`");
}

impl CollisionData {
    /// The body of `unit`'s collision, on the layer it names, by the mode's layers, or on the
    /// first; an error for a layer the mode does not declare.
    pub(crate) fn form(
        &self,
        cx: &BookCx<'_>,
        unit: &SourceType<'_>,
    ) -> Result<BodyForm, BookError> {
        let layer = match &self.layer {
            Some(name) => {
                let layers = cx.names_if::<Layer>()?;
                let named = layers.and_then(|layers| layers.named(name.as_str()));
                let unknown = || UnknownName::new(Layer::NAMES, name);
                let at = || BookPlace::UnitType(unit.place());
                named.ok_or_else(|| BookError::unknown(unit.package, at(), unknown()))?
            }
            None => Layer::FIRST,
        };
        Ok(BodyForm::new(self.shape, layer))
    }
}

impl Section for CollisionData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "collision";
}

/// A unit type's `variations`: each a body, of its own `collision`, one of which every spawn of the
/// type draws for its unit, in place of the type's own `collision`
/// (unit-variations.md, A unit type may vary its units).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VariationsData(pub Vec<VariationData>);

/// One of a unit type's `variations`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariationData {
    pub collision: CollisionData,
}

impl DataSection for VariationsData {
    const TABLE: DataTable = DataTable::new("A unit type's `variations`");
}

impl Section for VariationsData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "variations";
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;
    use campfire_geometry::MAX_BOUND;

    use super::*;

    #[test]
    fn a_collision_section_reads_its_height_from_0_to_the_bound() {
        let read = |text: &str| Toml::parse::<CollisionData>(text).map(|data| data.shape);
        let circle = BodyShape::circle(Num::ONE).unwrap();
        assert_eq!(read("radius = \"1\"").ok(), Some(circle));
        assert_eq!(
            read("radius = \"1\"\nheight = \"2.5\"").ok(),
            circle.raised(Num::HALF * 5)
        );
        assert_eq!(
            read("radius = \"1\"\nheight = \"2048\"").ok(),
            circle.raised(MAX_BOUND)
        );
        for height in ["-0.5", "2048.5"] {
            let text = format!("radius = \"1\"\nheight = \"{height}\"");
            assert!(read(&text).is_err(), "{height}");
        }
        assert_ne!(circle.raised(Num::ONE), Some(circle));
    }

    #[test]
    fn a_collision_section_writes_what_it_reads_back() {
        // A circle of no height and no layer writes its radius alone; a box its size, its height
        // and its layer.
        for (text, written) in [
            ("radius = \"1.5\"", "radius = \"1.5\"\n"),
            (
                "box = [\"4\", \"2\"]\nheight = \"3\"\nlayer = \"air\"",
                "box = [\"4\", \"2\"]\nheight = \"3\"\nlayer = \"air\"\n",
            ),
        ] {
            let data = Toml::parse::<CollisionData>(text).unwrap();
            assert_eq!(Toml::write(&data), written, "{text}");
            assert_eq!(Toml::parse::<CollisionData>(written).unwrap(), data);
        }
    }
}
