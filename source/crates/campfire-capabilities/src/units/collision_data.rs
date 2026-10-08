use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::units::body::BodyForm;
use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;

/// A unit type's `collision` section: its body, a circle of its `radius` or a box of its `box`
/// size, `[width, height]`, in meters, and the layer it moves on, the mode's first when it names
/// none. A type without one has no body: it collides with nothing, ranges count from its center,
/// and it moves on the first layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionData {
    /// Its body, on the first layer until `NavigationRules::form` puts it on its own.
    pub form: BodyForm,
    pub layer: Option<DeclaredName>,
}

/// A radius `Body::new` takes, or a size `BodyBox::new` takes, one of them, or the section fails
/// to read.
impl<'de> Deserialize<'de> for CollisionData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CollisionData, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            radius: Option<Scalar>,
            #[serde(rename = "box")]
            size: Option<[Scalar; 2]>,
            layer: Option<DeclaredName>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let form = match (fields.radius, fields.size) {
            (Some(radius), None) => BodyForm::circle(radius.checked("collision radius: ")?)
                .ok_or_else(|| D::Error::custom("a collision radius is positive, up to 64 m"))?,
            (None, Some([width, height])) => {
                let side = |side: Scalar| side.checked::<D::Error>("collision box: ");
                BodyForm::boxed([side(width)?, side(height)?]).ok_or_else(|| {
                    D::Error::custom(
                        "a collision box's sides are at least 2⁻¹⁰ m, its diagonal at most 126 m",
                    )
                })?
            }
            _ => {
                return Err(D::Error::custom(
                    "a collision `radius` or `box`, one of them",
                ));
            }
        };
        Ok(CollisionData {
            form,
            layer: fields.layer,
        })
    }
}
