use bevy_ecs::prelude::*;
use core::fmt::Debug;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum RollbackSystems {
    /// Decides whether this frame rolls back, and to which tick.
    Check,
    /// Re-enables the prediction-despawned entities, which the rollback may bring back.
    RestoreDespawned,
    /// Restores each predicted component to its state at the rollback tick and trims the
    /// histories, and despawns the pre-spawned and locally predicted entities that did not
    /// exist then.
    Prepare,
    /// Replays the ticks from the rollback tick to the current one.
    Rollback,
    /// Ends the rollback: starts each visual correction and removes the
    /// [`Rollback`](campfire_net_timeline::Rollback) resource.
    EndRollback,
    /// Decays each visual correction, from the value shown before the rollback to the corrected
    /// one.
    VisualCorrection,
}
