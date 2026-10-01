use serde::{Deserialize, Serialize};

use crate::actions::action_book::ActionId;
use crate::stats::modifier_book::ModifierId;

/// The scaling param a live stat change reads, computed again from its source at each refresh:
/// the param at `at` of its owner's run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LiveParam {
    pub(crate) owner: ParamOwner,
    pub(crate) at: u16,
}

/// Whose params a param is: a modifier's own, or those of the ability that applied it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ParamOwner {
    Modifier(ModifierId),
    Action(ActionId),
}
