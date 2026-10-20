use std::num::NonZeroU8;

use campfire_declarations::{PerType, Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use serde::{Deserialize, Serialize};

use crate::frame_point::FramePoint;

/// A unit type's `production` section: the most entries its train queue holds; where its
/// trained units come out, and where they go with no rally point set, each in its own frame
/// (04-capabilities/production.md, A producer may name its exit and rally).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionData {
    pub queue: NonZeroU8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<FramePoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rally: Option<FramePoint>,
}

impl PerType for ProductionData {
    const SCRIPTS_ONLY: bool = false;
}

impl DataSection for ProductionData {
    const TABLE: DataTable = DataTable::new("A unit type's `production`");
}

impl Section for ProductionData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "production";
}
