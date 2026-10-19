use campfire_common::DeclaredName;
use campfire_script::UnknownName;
use campfire_units::MapPointError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why a bot's due order goes unsent, to all of the units it commands or to some.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderUnsent {
    /// It is for the avatar, and the slot has none.
    NoAvatar,
    /// The slot owns no living unit of the type it names.
    NoUnit,
    /// A unit it commands holds no slot of the action it names.
    NoSlot,
    /// Its filter selects no living unit that may be a target.
    NoTarget,
}

/// Why a bot's script does not start in a match.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BotScriptError {
    /// It names a unit type, an action, an item type, a tag or a marker the match does not have.
    #[error(transparent)]
    Unknown(#[from] UnknownName),
    /// It names a marker of a region, which has no point.
    #[error("the marker {0} has no point")]
    NoPoint(DeclaredName),
    /// It names a point of the other metric's shape, or past the world's bound; one off the map
    /// is an order a player may give too, which the sim brings onto it.
    #[error("a point of the script fits no map of its metric")]
    Point(#[source] MapPointError),
}
