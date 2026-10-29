use campfire_common::DeclaredName;
use campfire_geometry::GridSearch;
use campfire_math::Num;
use campfire_mode::InputType;
use campfire_names::UnknownName;
use campfire_script::CallError;
use campfire_units::MapPointError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why a bot's due order goes unsent, to all of the units it commands or to some.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderUnsent {
    /// It is for the avatar, and the player slot has none.
    NoAvatar,
    /// The player slot owns no living unit of the type it names.
    NoUnit,
    /// A unit it commands holds no slot of the action it names.
    NoSlot,
    /// Its filter selects no living unit that may be a target.
    NoTarget,
    /// Its build finds no place within its search where the build passes.
    NoPlace,
    /// The unit it commands first carries no stack of the item type it names.
    NoStack,
}

/// Why a bot's script does not start in a match, or why the orders a think gave name what the
/// match lacks.
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
    /// It names a search for a place of a gap not above 0.
    #[error("a search for a place of gap {0}, not above 0")]
    NearGap(Num),
    /// It names a search for a place of more steps than a search reaches.
    #[error("a search for a place of {0} steps, past the {max} a search reaches", max = GridSearch::MAX_STEPS)]
    NearSteps(u32),
    /// It sends a mode input of a name the mode declares no input of.
    #[error("the mode declares no input {0}")]
    NoInput(DeclaredName),
    /// It sends a mode input a value of another type than the mode declares for it.
    #[error("the mode input {name} takes a {declared:?}")]
    InputType {
        name: DeclaredName,
        declared: InputType,
    },
}

/// Why a think gave no orders: its call failed, or an order it gave names what the match lacks.
#[derive(Debug, Error)]
pub enum ThinkError {
    #[error(transparent)]
    Call(#[from] CallError),
    #[error(transparent)]
    Names(#[from] BotScriptError),
}
