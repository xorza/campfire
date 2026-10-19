use crate::locally_predicted::LocallyPredicted;
use crate::prediction_despawned::PredictionDespawned;
use crate::prediction_manager::PredictionManager;
use bevy_ecs::error;
use bevy_ecs::prelude::*;
use campfire_net_despawn::DespawnReplicated;
use campfire_net_replication::PreSpawnedReceiver;
use campfire_net_sync::InputTimelineConfig;
use campfire_net_timeline::LocalTimeline;

/// Permanently remove locally predicted entities once their despawn tick is outside the rollback
/// window.
pub(crate) fn finalize_local_despawns(
    mut commands: Commands<'_, '_>,
    timeline: Res<'_, LocalTimeline>,
    prediction_manager: Res<'_, PredictionManager>,
    input_config: Res<'_, InputTimelineConfig>,
    query: Query<
        '_,
        '_,
        (Entity, &PredictionDespawned),
        (With<LocallyPredicted>, Allow<PredictionDespawned>),
    >,
) {
    let oldest = prediction_manager.oldest_rollback_tick(timeline.tick(), &input_config);
    for (entity, disabled) in &query {
        // Keep the entity while its disable tick can still be reached by an accepted rollback.
        // Once it is older than the same effective bound enforced by `check_rollback`, no later
        // reconciliation can restore it.
        if disabled.tick < oldest {
            commands
                .entity(entity)
                .queue_handled(DespawnReplicated, error::warn);
        }
    }
}

/// Forgets the pre-spawned entities no rollback can reach, so the list a rollback walks holds only
/// those its window covers.
pub(crate) fn forget_unreachable_pre_spawns(
    mut receiver: ResMut<'_, PreSpawnedReceiver>,
    timeline: Res<'_, LocalTimeline>,
    prediction_manager: Res<'_, PredictionManager>,
    input_config: Res<'_, InputTimelineConfig>,
) {
    receiver.forget_rollback_through(
        prediction_manager.oldest_rollback_tick(timeline.tick(), &input_config),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;
    use campfire_net_core::NetTick;

    /// With a rollback window of `w` ticks at tick 100, an entity disabled at `100 - w - 1` can be
    /// restored by no rollback and is despawned, while one disabled at `100 - w`, the oldest tick a
    /// rollback restores, stays. A pre-spawn at `100 - w` is forgotten, as a rollback despawns only
    /// what spawned after the tick it restores, and one at `100 - w + 1` stays listed.
    #[test]
    fn a_local_despawn_ends_once_it_leaves_the_rollback_window() {
        let mut world = World::new();
        world.init_resource::<PredictionManager>();
        world.init_resource::<InputTimelineConfig>();
        let mut timeline = LocalTimeline::default();
        timeline.shift(100);
        world.insert_resource(timeline);
        let window = u32::from(
            world
                .resource::<PredictionManager>()
                .max_rollback_ticks(world.resource::<InputTimelineConfig>()),
        );
        let mut disabled_at = |tick| {
            world
                .spawn((LocallyPredicted::default(), PredictionDespawned { tick }))
                .id()
        };
        let gone = disabled_at(NetTick(100 - window - 1));
        let kept = disabled_at(NetTick(100 - window));

        world.run_system_once(finalize_local_despawns).unwrap();

        assert!(world.get_entity(gone).is_err());
        assert!(world.get_entity(kept).is_ok());

        let mut receiver = PreSpawnedReceiver::default();
        receiver.register(NetTick(100 - window), gone);
        receiver.register(NetTick(100 - window + 1), kept);
        world.insert_resource(receiver);
        world
            .run_system_once(forget_unreachable_pre_spawns)
            .unwrap();
        assert_eq!(
            world.resource::<PreSpawnedReceiver>().rollback(),
            [(NetTick(100 - window + 1), kept)]
        );
    }
}
