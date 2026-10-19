use campfire_values::ParamRef;
use serde::{Deserialize, Serialize};

use crate::action_range::ActionRange;

/// A range as data writes it: a range, or `{ param = "<name>" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RangeField {
    Range(ActionRange),
    Param(ParamRef),
}
