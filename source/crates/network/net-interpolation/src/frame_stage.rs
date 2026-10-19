use bevy_ecs::schedule::SystemSet;

/// The phases of each frame interpolation set, in order: the holders resolve the archetypes, then
/// the typed systems run.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub(crate) enum FrameStage {
    Refresh,
    Run,
}
