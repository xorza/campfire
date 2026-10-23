use bevy_ecs::schedule::SystemSet;

/// The phases of each frame interpolation set, in order: the holders resolve the archetypes, then
/// the typed systems run; in `PostUpdate` the holders then resolve the archetypes the writes made,
/// as a visual a write puts in moves its entity, for the systems that read where the visuals are
/// after them.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub(crate) enum FramePhaseSystems {
    Refresh,
    Run,
    Settle,
}
