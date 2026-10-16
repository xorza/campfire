use campfire_values::ActionId;

use crate::launch_id::LaunchId;
use crate::list_hook::ListHook;

/// Which list a call runs, before which hook: an action's, by any of its hooks, or one a launch
/// holds, by its delivery's hit or end alone, as a launch resolves nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListsOf {
    Action(ActionId, ListHook),
    Launch(LaunchId, ReachHook),
}

/// The hook a delivery's list runs before: its hit's, or its end's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReachHook {
    OnHit,
    OnEnd,
}

impl ReachHook {
    /// Its place among a launch's lists.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::OnHit => 0,
            Self::OnEnd => 1,
        }
    }
}

impl From<ReachHook> for ListHook {
    fn from(hook: ReachHook) -> Self {
        match hook {
            ReachHook::OnHit => Self::OnHit,
            ReachHook::OnEnd => Self::OnEnd,
        }
    }
}
