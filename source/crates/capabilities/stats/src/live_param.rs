use campfire_values::{ActionId, ModifierId};
use serde::{Deserialize, Serialize};

/// The scaling param a live stat change reads, computed again from its source at each refresh:
/// the param at `at` of its owner's run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveParam {
    pub owner: ParamOwner,
    pub at: u16,
}

/// Whose params a param is: a modifier's own, or those of the ability that applied it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamOwner {
    Modifier(ModifierId),
    Action(ActionId),
}
