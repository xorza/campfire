use campfire_declarations::PerId;
use campfire_values::{ActionId, ModifierId};

use crate::research_data::ResearchHolder;

/// What a research action runs: the modifier it gives, and who holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchSpec {
    pub modifier: ModifierId,
    pub holder: ResearchHolder,
}

impl PerId<ActionId> for ResearchSpec {
    const SCRIPTS_ONLY: bool = false;
}
