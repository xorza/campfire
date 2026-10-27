#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A table of the units `units` selects, at `detail`, of at most `limit` rows from the row `first`,
/// counted from 0, so a match of many units comes in pages (02-engine-core.md, A table comes in
/// pages).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UnitsAsk {
    pub units: UnitSelection,
    pub detail: Detail,
    #[serde(default)]
    pub first: u32,
    pub limit: Option<u32>,
}

/// The units a table selects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnitSelection {
    All,
    /// Those a player slot owns, counted from 0, as the table names them.
    PlayerSlot(u32),
    Team(u8),
    /// Those of a type, by the name the mode's books give it.
    Type(String),
    /// Those of these stable ids, in any order.
    Ids(Vec<u64>),
    /// Those in a probe state.
    State(UnitActivity),
}

/// What a unit does, as the probe reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnitActivity {
    Idle,
    /// Walks, stuck or not.
    Walking,
    /// Walks, and its walk made no progress for longer than steering waits.
    Stuck,
    Working,
    Fighting,
    Acting,
}

/// A table's columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Detail {
    /// A unit's id, type, player slot, place, what it does and its life.
    Concise,
    /// Every column the probe writes.
    Detailed,
}
