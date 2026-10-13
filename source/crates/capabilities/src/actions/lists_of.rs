use crate::actions::launch_id::LaunchId;
use crate::units::action_id::ActionId;

/// Whose lists a call runs: an action's, or those a launch holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListsOf {
    Action(ActionId),
    Launch(LaunchId),
}
