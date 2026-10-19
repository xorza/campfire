use serde::{Deserialize, Serialize};

/// How a path's walkers go along it past its last waypoint: they stay there, go on to the first
/// again, as a closed patrol, or turn and walk it back, as a guard's beat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathWalk {
    #[default]
    Once,
    Loop,
    BackAndForth,
}
