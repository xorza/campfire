use serde::{Deserialize, Serialize};

/// How a player of the mode controls its units, as its `[players] control` says: by a selection
/// of its own units, or by its avatar alone (02-engine-core.md, The mode says what a player
/// selects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerControl {
    /// A click selects one of the player's units and a drag those in its rectangle; an order
    /// goes to the selection.
    #[default]
    Units,
    /// The player selects nothing, and each order goes to its avatar.
    Avatar,
}
