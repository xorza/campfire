use campfire_core::ActionId;

use crate::launch_id::LaunchId;

/// Whose lists a call runs: an action's, or those a launch holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListsOf {
    Action(ActionId),
    Launch(LaunchId),
}
