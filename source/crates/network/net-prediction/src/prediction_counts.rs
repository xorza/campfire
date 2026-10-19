use bevy_ecs::{reflect::ReflectResource, resource::Resource};
use bevy_reflect::Reflect;

/// The client's rollback counts, which the diagnostics plugin flushes periodically.
#[derive(Default, Resource, Debug, Reflect)]
#[reflect(Resource)]
pub struct PredictionCounts {
    /// Incremented once per rollback
    pub rollbacks: u32,
    /// Per rollback, incremented by the number of ticks the rollback window contains
    pub rollback_ticks: u32,
}
