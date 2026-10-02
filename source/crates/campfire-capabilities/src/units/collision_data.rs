use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::units::body::Body;
use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;

/// A unit type's `collision` section: the radius of its body, in meters, and the layer it moves
/// on, the mode's first when it names none. A type without one has no body: it collides with
/// nothing, ranges count from its center, and it moves on the first layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionData {
    /// Its body, on the first layer until `NavigationRules::body` puts it on its own.
    pub body: Body,
    pub layer: Option<DeclaredName>,
}

/// A radius `Body::new` takes, or the section fails to read.
impl<'de> Deserialize<'de> for CollisionData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CollisionData, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            radius: Scalar,
            layer: Option<DeclaredName>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let body = fields
            .radius
            .to_num()
            .and_then(Body::new)
            .ok_or_else(|| D::Error::custom("a collision radius is positive, up to 64 m"))?;
        Ok(CollisionData {
            body,
            layer: fields.layer,
        })
    }
}
