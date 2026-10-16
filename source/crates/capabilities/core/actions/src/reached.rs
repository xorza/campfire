use campfire_sim::StableId;

use crate::lists_of::ReachHook;

/// What a delivery or a shot reached: a unit it hit, or its end, by a hit or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reached {
    Hit(StableId),
    End,
}

impl Reached {
    /// The hook whose list runs for it.
    pub const fn hook(self) -> ReachHook {
        match self {
            Self::Hit(_) => ReachHook::OnHit,
            Self::End => ReachHook::OnEnd,
        }
    }

    /// The unit it hit, if any.
    pub const fn unit(self) -> Option<StableId> {
        match self {
            Self::Hit(unit) => Some(unit),
            Self::End => None,
        }
    }
}
