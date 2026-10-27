use campfire_declarations::{Section, SectionTable};
use campfire_math::Num;
use campfire_script::{ApiVersion, Status};
use campfire_units::MapPoint;
use serde::{Deserialize, Serialize};

/// A map's `[grid]`: the size of the square cells vision reveals, in meters, its brush, whether the
/// map starts shrouded, so each team keeps the cells it explored (04-capabilities/vision.md, A team
/// keeps the cells it explored), and whether a team sees the units that do not walk of a team it
/// regards as neutral where it explored (04-capabilities/vision.md, A neutral building stands in
/// the fog).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridData {
    pub cell: Num,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub brush: Vec<BrushData>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shroud: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub neutral_in_fog: bool,
}

/// A brush, `[[grid.brush]]`: the polygon of `points`, whose cells only a unit in it reveals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrushData {
    pub points: Vec<MapPoint>,
}

impl Section for GridData {
    const TABLE: SectionTable = SectionTable::Map;
    const KEY: &'static str = "grid";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

#[cfg(test)]
mod tests;
