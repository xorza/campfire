use bevy_ecs::schedule::SystemSet;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum InterpolationSystems {
    /// Update component histories and apply stack-held interpolation.
    ///
    /// This runs in two ordered phases. The first phase updates histories and
    /// applies pending component insertions/removals at the interpolation
    /// timeline. Deferred commands are flushed before the second phase writes
    /// interpolated values for rules that enabled the apply phase.
    ///
    /// This can be in Update since we use the confirmed.tick to add values to the history, which is
    /// independent from the local tick.
    Prepare,
    /// Run user interpolation systems after the stack has prepared histories.
    ///
    /// Use this set for the history-only interpolation rules registered with
    /// `InterpolationFns::history_only`.
    Interpolate,

    /// SystemSet encompassing all other interpolation sets
    All,
}
