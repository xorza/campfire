use campfire_sim::StableId;

use crate::actions::action_book::ActionId;
use crate::deliveries::hit::Hit;
use crate::scripts::hook::Hook;

/// A hit of the unit `reached`, `on_hit`, or an end, `on_end`, of a delivery of `source`'s
/// `action` at `rank`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Delivered {
    pub(crate) source: StableId,
    pub(crate) action: ActionId,
    pub(crate) rank: u8,
    pub(crate) hook: Hook,
    pub(crate) reached: Option<StableId>,
    pub(crate) hit: Hit,
}
