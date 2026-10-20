use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use campfire_values::FilterData;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

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
    pub inside: AreaInsideData,
}

/// The modifiers of its package an area holds on the units inside it: on its source, on the
/// source's other allies, and on the units that may be attacked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaInsideData {
    #[serde(rename = "self")]
    pub caster: Option<DeclaredName>,
    pub allies: Option<DeclaredName>,
    pub enemies: Option<DeclaredName>,
}

impl AreaInsideData {
    /// The ids of the modifiers it names.
    pub fn modifiers(&self) -> impl Iterator<Item = &DeclaredName> + '_ {
        [&self.caster, &self.allies, &self.enemies]
            .into_iter()
            .flatten()
    }
}

/// Data is untrusted, so a negative radius fails to read.
impl<'de> Deserialize<'de> for AreaData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            radius: Num,
            #[serde(default)]
            delay_ms: u64,
            #[serde(default)]
            duration_ms: u64,
            affects: Option<FilterData>,
            #[serde(default)]
            inside: AreaInsideData,
        }
        let fields = Fields::deserialize(deserializer)?;
        if fields.radius < Num::ZERO {
            return Err(D::Error::custom("an area's radius is not negative"));
        }
        Ok(Self {
            radius: fields.radius,
            delay_ms: fields.delay_ms,
            duration_ms: fields.duration_ms,
            affects: fields.affects,
            inside: fields.inside,
        })
    }
}

impl DataSection for AreaData {
    const TABLE: DataTable = DataTable::new("A unit type's `area`");
}

impl DataSection for AreaInsideData {
    const TABLE: DataTable = DataTable::new("An area's `inside`");
}

impl Section for AreaData {
    const TABLE: SectionTable = SectionTable::DeliveryType;
    const KEY: &'static str = "area";
}
