use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_core::NetTick;

/// Disables a predicted entity that a prediction despawn removed, so that a rollback to before
/// `tick` can bring it back.
#[derive(Component, PartialEq, Eq, Debug, Reflect)]
#[reflect(Component)]
pub struct PredictionDespawned {
    /// NetTick at which the entity was prediction-despawned.
    ///
    /// A locally predicted entity is retained until this tick falls outside the effective
    /// rollback window, at which point the entity can be permanently reclaimed.
    pub tick: NetTick,
}
