use bevy_ecs::schedule::SystemSet;
use campfire_sim::SimSystems;

/// A stage with its closing set, which a condition on the stages stops together: the closing set
/// belongs to the stage before it, so it runs when its stage ran, the stage that the condition
/// turned false in included.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct StageSpanSystems(pub(super) SimSystems);
