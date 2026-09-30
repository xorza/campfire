use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};

use crate::combat::combat_data::CombatData;
use crate::orders::ai_data::AiData;
use crate::stats::stats_data::StatsData;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::scalar::Scalar;

/// The mode's `data/units.toml`: its unit types, by name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitsData {
    pub units: BTreeMap<String, UnitTypeFile>,
}

/// A unit type as its data file declares it: the core's tags and params, and a section for each
/// capability the type uses, in one table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitTypeFile {
    pub core: UnitTypeData,
    pub stats: Option<StatsData>,
    pub combat: Option<CombatData>,
    pub orders: Option<AiData>,
    pub vision: Option<VisionData>,
}

/// A unit type's `vision` section. The release loads it, and runs none of it yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisionData {
    /// It sees stealthed units.
    #[serde(default)]
    pub true_sight: bool,
}

/// The flat table of a unit type, the core's fields among the capabilities' sections.
impl<'de> Deserialize<'de> for UnitTypeFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<UnitTypeFile, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            tags: Vec<String>,
            #[serde(default)]
            params: BTreeMap<String, Scalar>,
            stats: Option<StatsData>,
            combat: Option<CombatData>,
            orders: Option<AiData>,
            vision: Option<VisionData>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Ok(UnitTypeFile {
            core: UnitTypeData {
                tags: fields.tags,
                params: fields.params,
            },
            stats: fields.stats,
            combat: fields.combat,
            orders: fields.orders,
            vision: fields.vision,
        })
    }
}
