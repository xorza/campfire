use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::filter_data::FilterData;
use crate::values::scalar::Scalar;

/// A unit type's `area` section, which makes its units areas: they reach the units `affects`
/// selects, `enemies` by default, whose bodies come within `radius` meters, once `delay_ms` after
/// they land; they last `duration_ms`, and hold the modifiers `inside` names on the units inside
/// meanwhile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaData {
    pub radius: Num,
    pub delay_ms: u64,
    pub duration_ms: u64,
    pub affects: Option<FilterData>,
    pub inside: AreaInside,
}

/// The modifiers of its package an area holds on the units inside it: on its source, on the
/// source's other allies, and on the units that may be attacked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaInside {
    #[serde(rename = "self")]
    pub caster: Option<String>,
    pub allies: Option<String>,
    pub enemies: Option<String>,
}

impl AreaInside {
    /// The ids of the modifiers it names.
    pub fn modifiers(&self) -> impl Iterator<Item = &str> + '_ {
        [&self.caster, &self.allies, &self.enemies]
            .into_iter()
            .flatten()
            .map(String::as_str)
    }
}

/// Data is untrusted, so a negative radius fails to read.
impl<'de> Deserialize<'de> for AreaData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<AreaData, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            radius: Scalar,
            #[serde(default)]
            delay_ms: u64,
            #[serde(default)]
            duration_ms: u64,
            affects: Option<FilterData>,
            #[serde(default)]
            inside: AreaInside,
        }
        let fields = Fields::deserialize(deserializer)?;
        let radius = fields
            .radius
            .to_num()
            .filter(|&radius| radius >= Num::ZERO)
            .ok_or_else(|| D::Error::custom("an area's radius is not negative"))?;
        Ok(AreaData {
            radius,
            delay_ms: fields.delay_ms,
            duration_ms: fields.duration_ms,
            affects: fields.affects,
            inside: fields.inside,
        })
    }
}
