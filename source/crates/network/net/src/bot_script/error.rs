use serde::{Deserialize, Serialize};

/// Why a bot's order commands no unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitsMissing {
    /// It is for the avatar, and the slot has none.
    NoAvatar,
    /// The slot owns no living unit of the type it names.
    NoUnit,
}
