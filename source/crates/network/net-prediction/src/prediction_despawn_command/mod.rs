use crate::locally_predicted::LocallyPredicted;
use crate::prediction_despawned::PredictionDespawned;
use crate::prediction_gate;
use crate::prediction_setup::PredictionSetup;
use bevy_ecs::prelude::*;
use campfire_net_connection::NetworkingState;
use campfire_net_despawns::DespawnReplicated;
use campfire_net_replication::PreSpawned;
use campfire_net_replication::Predicted;
use campfire_net_timeline::LocalTimeline;
use tracing::debug;

/// Despawns a predicted entity as a rollback can undo: the entity is disabled with
/// [`PredictionDespawned`] at the tick the fixed schedule simulates, its local timeline's tick
/// outside it, and stays so until confirmed replication resolves
/// it or, for a locally predicted entity, until its despawn tick leaves the rollback window. A
/// rollback to before that tick re-enables it, and the replay may despawn it again. Without
/// prediction there is no rollback, so the entity is despawned at once.
#[derive(Debug)]
pub struct PredictionDespawnCommand {
    pub(crate) entity: Entity,
}

impl Command for PredictionDespawnCommand {
    type Out = ();

    /// # Panics
    ///
    /// While the app predicts, when the entity is not predicted: only a predicted entity can be
    /// prediction-despawned.
    fn apply(self, world: &mut World) -> Self::Out {
        let predicting = world
            .get_resource::<NetworkingState>()
            .is_some_and(|networking| {
                prediction_gate::predicts(world.contains_resource::<PredictionSetup>(), networking)
            });
        let timeline = world.get_resource::<LocalTimeline>();
        let ticks = timeline.map(|timeline| (timeline.simulated_tick(), timeline.tick()));
        let Ok(mut entity) = world.get_entity_mut(self.entity) else {
            return;
        };
        // Without prediction there is no rollback, so the entity is despawned at once, and its
        // replicated children stay, as their server holds their life.
        let Some((simulated, tick)) = ticks.filter(|_| predicting) else {
            DespawnReplicated.apply(entity);
            return;
        };
        // A rollback's replay runs only the fixed schedule, so it never despawns again an entity
        // despawned outside it: a locally predicted one, which this end alone holds, goes at
        // once, as no rollback must bring it back; one the server holds, or a pre-spawn that
        // waits for its match or its expiry, waits as inside it.
        let local_only = entity.contains::<LocallyPredicted>()
            && !entity.contains::<Predicted>()
            && !entity.contains::<PreSpawned>();
        let Some(disabled_at) = simulated.or_else(|| (!local_only).then_some(tick)) else {
            DespawnReplicated.apply(entity);
            return;
        };
        assert!(
            entity.contains::<Predicted>()
                || entity.contains::<LocallyPredicted>()
                // see https://github.com/cBournhonesque/lightyear/issues/818
                || entity.contains::<PreSpawned>(),
            "prediction_despawn is only for predicted entities, and {:?} is not one",
            self.entity
        );
        // Do not despawn predicted entities immediately. A replicated predicted entity remains
        // disabled until confirmed replication resolves it; a locally predicted entity remains
        // disabled until its despawn tick falls outside the rollback window.
        debug!(?self.entity, "inserting prediction disable marker");
        entity.insert(PredictionDespawned { tick: disabled_at });
    }
}

#[cfg(test)]
mod tests;
