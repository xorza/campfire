use std::collections::BTreeMap;

use serde::Deserialize;

use crate::combat::combat_data::CombatData;
use crate::control::ai_data::AiData;
use crate::stats::stats_data::StatsData;
use crate::units::scalar::Scalar;
use crate::units::unit_type_data::UnitTypeData;

/// The mode's `data/units.toml`: its unit types, by name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitsData {
    pub units: BTreeMap<String, UnitTypeFile>,
}

/// A unit type as its data file declares it: the core's tags and params, and a section for each
/// capability the type uses.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitTypeFile {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub params: BTreeMap<String, Scalar>,
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

impl UnitTypeFile {
    /// Its core fields.
    pub fn core(&self) -> UnitTypeData {
        UnitTypeData {
            tags: self.tags.clone(),
            params: self.params.clone(),
        }
    }
}
