use campfire_declarations::{Section, SectionTable};
use serde::{Deserialize, Serialize};

use crate::ground::{CellSplit, CoarseSpan};

/// A map's `[ground]` (04-capabilities/00-overview.md, The sim's ground is the map's height grid):
/// the diagonal each cell of its height grid splits on, and its coarse cells, the grid's own when
/// it names none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub split: CellSplit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<CoarseSpan>,
}

impl Section for GroundData {
    const TABLE: SectionTable = SectionTable::Map;
    const KEY: &'static str = "ground";
}

/// Whether `split` is the split a read takes when the field is left out.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's `skip_serializing_if` passes the field by reference"
)]
fn is_default(split: &CellSplit) -> bool {
    *split == CellSplit::default()
}
