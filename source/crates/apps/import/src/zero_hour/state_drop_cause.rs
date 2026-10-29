use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops a look's condition states: its draw module draws no model in any of them,
/// so the look holds no draw of it; the package lacks the model a default state of its object
/// names; or a state holds a stride, a speed or a rise the engine does not hold. Of the last two
/// the object has no look, which the client shows as the engine's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum StateDropCause {
    Undrawn,
    Missing,
    Number,
}
