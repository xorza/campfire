use std::num::NonZeroU8;

use serde::Deserialize;

/// A unit type's `production` section: the most entries its train queue holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionData {
    pub queue: NonZeroU8,
}
