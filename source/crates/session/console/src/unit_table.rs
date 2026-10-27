#![expect(
    clippy::disallowed_methods,
    reason = "a derive of JsonSchema calls serde_json::to_value"
)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A table of units as the probe aligns it, for a person to read: the counts of each player
/// slot's units, then a row for each of `shown` units from the row `first`, of the `selected`
/// units the filter selects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UnitTable {
    pub text: String,
    pub selected: u32,
    pub first: u32,
    pub shown: u32,
}
