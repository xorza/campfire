use bevy_ecs::prelude::*;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum PredictionSystems {
    /// In `PreUpdate`: the sets of [`RollbackSystems`](crate::rollback_systems::RollbackSystems).
    Rollback,
    /// In `FixedPreUpdate`, during a rollback replay: snaps each component to its confirmed
    /// value at the replayed tick, where one exists.
    SnapToConfirmed,
    /// In `FixedPostUpdate`: despawns the locally predicted entities whose prediction despawn
    /// left the rollback window.
    EntityDespawn,
    /// In `FixedPostUpdate`, after the simulation step: records the predicted history.
    UpdateHistory,
    /// Every other set of prediction.
    All,
}
