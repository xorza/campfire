use std::num::NonZeroU8;

use crate::declare::per_type::PerType;
use campfire_sim::Capability;
use serde::Deserialize;

/// A unit type's `production` section: the most entries its train queue holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionData {
    pub queue: NonZeroU8,
}

impl PerType for ProductionData {
    const CAPABILITY: Option<Capability> = Some(Capability::Production);
    const SCRIPTS_ONLY: bool = false;
}
