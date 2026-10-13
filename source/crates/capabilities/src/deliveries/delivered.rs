use campfire_sim::StableId;

use crate::deliveries::delivering::Delivering;
use crate::scripts::hook::Hook;
use crate::values::hit::Hit;

/// What a delivery `by` reached, as `hit` says: a unit it hit, whose hook is `on_hit`, or its end,
/// whose hook is `on_end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Delivered {
    pub(crate) by: Delivering,
    pub(crate) reach: Reached,
    pub(crate) hit: Hit,
}

/// What a delivery reached: a unit, or its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reached {
    Hit(StableId),
    End,
}

impl Reached {
    /// The hook that runs for it.
    pub(crate) const fn hook(self) -> Hook {
        match self {
            Reached::Hit(_) => Hook::OnHit,
            Reached::End => Hook::OnEnd,
        }
    }

    /// The unit it hit, if any.
    pub(crate) const fn unit(self) -> Option<StableId> {
        match self {
            Reached::Hit(unit) => Some(unit),
            Reached::End => None,
        }
    }
}
