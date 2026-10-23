use campfire_common::PlayerSlot;
use serde::{Deserialize, Serialize};

use crate::team::Team;

/// Whose side a unit acts for: its team, and the player who owns it, if one does. A delivery and
/// a shot keep their source's as it launched, so what they spawn or claim as they land outlives
/// their source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allegiance {
    pub team: Team,
    pub owner: Option<PlayerSlot>,
}
