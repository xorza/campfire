use campfire_declarations::{Section, SectionTable};
use campfire_math::Num;
use campfire_script::{ApiVersion, Status};
use campfire_script::{DataSection, DataTable};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::sight::Sight;

/// A unit type's `vision` section: how far it sees, in meters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VisionData {
    #[serde(rename = "sight_range")]
    pub sight: Sight,
}

/// A sight range that is not negative, or the section fails to read.
impl<'de> Deserialize<'de> for VisionData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            sight_range: Num,
        }
        let fields = Fields::deserialize(deserializer)?;
        let sight = Sight::new(fields.sight_range)
            .ok_or_else(|| D::Error::custom("a sight range is a number that is not negative"))?;
        Ok(Self { sight })
    }
}

impl DataSection for VisionData {
    const TABLE: DataTable = DataTable::new("A unit type's `vision`");
}

impl Section for VisionData {
    const TABLE: SectionTable = SectionTable::AnyType;
    const KEY: &'static str = "vision";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
