use campfire_declarations::{Section, SectionTable};
use campfire_math::Num;
use campfire_units::MapPoint;
use serde::{Deserialize, Serialize};

/// A map's `[grid]`: the size of the square cells vision reveals, in meters, and its brush.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridData {
    pub cell: Num,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub brush: Vec<BrushData>,
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
}

#[cfg(test)]
mod tests;
