use bevy_ecs::prelude::*;

/// The stages of [`RollbackSystems::Check`](crate::rollback_systems::RollbackSystems::Check), in
/// order, each component's survey and scan a system of its own.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub(crate) enum RollbackCheckStage {
    /// Selects the candidate checkpoint the policy looks at this frame.
    Begin,
    /// Each predicted component reports its check targets and its pending diffs.
    Survey,
    /// The policy decides on a forced rollback, a rollback on each confirmed state, or a scan.
    Decide,
    /// Each predicted component compares its states at the scan's checkpoint.
    Scan,
    /// Asks for the rollback the frame decided on.
    Finish,
}
