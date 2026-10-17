use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::body_form::BodyForm;

/// A unit type's `collision` section: its body, a circle of its `radius` or a box of its `box`
/// size, `[width, depth]`, in meters, its `height`, 0 when it names none, and the layer it moves
/// on, the mode's first when it names none. A type without one has no body: it collides with nothing, ranges count from its center,
/// and it moves on the first layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionData {
    /// Its body, on the first layer until `ModeNavigationData::form` puts it on its own.
    pub form: BodyForm,
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
        let form = match (fields.radius, fields.size) {
            (Some(radius), None) => BodyForm::circle(radius)
                .ok_or_else(|| D::Error::custom("a collision radius is positive, up to 2,048 m"))?,
            (None, Some(size)) => BodyForm::box_sized(size).ok_or_else(|| {
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
        let form = form
            .raised(fields.height)
            .ok_or_else(|| D::Error::custom("a collision height is at least 0, up to 2,048 m"))?;
        Ok(Self {
            form,
            layer: fields.layer,
        })
    }
}

impl DataSection for CollisionData {
    const TABLE: DataTable = DataTable::Collision;
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;
    use campfire_geometry::MAX_BOUND;

    use super::*;

    #[test]
    fn a_collision_section_reads_its_height_from_0_to_the_bound() {
        let read = |text: &str| Toml::parse::<CollisionData>(text).map(|data| data.form);
        let circle = BodyForm::circle(Num::ONE).unwrap();
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
}
