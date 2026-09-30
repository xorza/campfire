use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::units::body::Body;
use crate::values::scalar::Scalar;

/// A unit type's `collision` section: the radius of its body, in meters. A type without one has
/// no body: it collides with nothing, and ranges count from its center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionData {
    pub body: Body,
}

/// A radius `Body::new` takes, or the section fails to read.
impl<'de> Deserialize<'de> for CollisionData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CollisionData, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            radius: Scalar,
        }
        let fields = Fields::deserialize(deserializer)?;
        let body = fields
            .radius
            .to_num()
            .and_then(Body::new)
            .ok_or_else(|| D::Error::custom("a collision radius is positive, up to 64 m"))?;
        Ok(CollisionData { body })
    }
}
