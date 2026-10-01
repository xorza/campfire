use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::scalar::Scalar;
use crate::vision::sight::Sight;

/// A unit type's `vision` section: how far it sees, in meters, and whether it sees stealthed
/// units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisionData {
    pub sight: Sight,
    pub true_sight: bool,
}

/// A sight range that is not negative, or the section fails to read.
impl<'de> Deserialize<'de> for VisionData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<VisionData, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            sight_range: Scalar,
            #[serde(default)]
            true_sight: bool,
        }
        let fields = Fields::deserialize(deserializer)?;
        let sight = fields
            .sight_range
            .to_num()
            .and_then(Sight::new)
            .ok_or_else(|| D::Error::custom("a sight range is a number that is not negative"))?;
        Ok(VisionData {
            sight,
            true_sight: fields.true_sight,
        })
    }
}
