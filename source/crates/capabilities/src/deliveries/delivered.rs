use campfire_sim::StableId;

use crate::scripts::hook::Hook;
use crate::units::action_id::ActionId;
use crate::values::hit::Hit;

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
