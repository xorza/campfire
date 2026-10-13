use serde::Deserialize;

use crate::actions::action_range::ActionRange;
use crate::values::number::ParamRef;

/// A range as data writes it: a range, or `{ param = "<name>" }`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum RangeField {
    Range(ActionRange),
    Param(ParamRef),
}
