use campfire_core::Hook;
use campfire_sim::StableId;
use campfire_values::Hit;

use crate::delivering::Delivering;

/// What a delivery `by` reached, as `hit` says: a unit it hit, whose hook is `on_hit`, or its end,
/// whose hook is `on_end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivered {
    pub by: Delivering,
    pub reach: Reached,
    pub hit: Hit,
}

/// What a delivery reached: a unit, or its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reached {
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
    pub const fn unit(self) -> Option<StableId> {
        match self {
            Reached::Hit(unit) => Some(unit),
            Reached::End => None,
        }
    }
}
