use bevy_ecs::schedule::SystemSet;

/// The phases of
/// [`InterpolationSystems::Prepare`](crate::interpolation_systems::InterpolationSystems::Prepare),
/// in order: the owners resolve the archetypes before each typed phase, as the history phase's
/// insertions and removals create archetypes the apply phase reads.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub(crate) enum InterpolationStage {
    RefreshHistory,
    /// Each maintained component's history, and its live presence at the interpolation tick.
    History,
    RefreshApply,
    /// Each rule writes its interpolated values.
    Apply,
}
