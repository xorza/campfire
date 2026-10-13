use serde::{Deserialize, Serialize};

/// A launch of an effect list, by its place among the match's launches, whose own lists its area
/// runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct LaunchId(pub(super) u32);
