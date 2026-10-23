use crate::locally_predicted::LocallyPredicted;
use crate::prediction_history::PredictionHistory;
use crate::prediction_setup::PredictionSetup;
use crate::rollback_plugin::RollbackPlugin;
use crate::rollback_scan::RollbackScan;
use bevy_ecs::error;
use campfire_net_despawns::DespawnReplicated;
use campfire_net_replication::PreSpawnedReceiver;
use campfire_net_sync::InputTimelineSetup;
use campfire_net_timeline::LocalTimeline;

use crate::disabled_during_rollback::DisabledDuringRollback;

use crate::prediction_despawned::PredictionDespawned;

use crate::prediction_registry;
use crate::prediction_systems::PredictionSystems;
use crate::rollback_check_systems::RollbackCheckSystems;

use crate::rollback_systems::RollbackSystems;
use bevy_app::prelude::*;
use bevy_ecs::component::Mutable;
use bevy_ecs::entity_disabling::DefaultQueryFilters;
use bevy_ecs::prelude::*;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::ReplicationSystems;
use campfire_net_timeline::Rollback;

/// A client's prediction and rollback, which run while the app holds a
/// [`PredictionSetup`](crate::prediction_setup::PredictionSetup) and its topology is a client; a
/// history writer also waits for a synced timeline, so it records no value under a tick before the
/// sync. A server stays authoritative and runs none of it.
#[derive(Debug, Default)]
pub(crate) struct PredictionPlugin;

// Higher-priority markers win when more than one applies to an update.
// Pre-spawn must preserve its specialized reconciliation semantics before the
// general prediction markers are considered. Pre-spawn is
// strongest because a locally predicted absence must not be materialized by a
// lower-priority initial-write path.
pub(crate) const PRE_SPAWNED_PRIORITY: usize = 120;
pub(crate) const PREDICTED_SEND_PRIORITY: usize = 100;
pub(crate) const PREDICTED_PRIORITY: usize = 90;

/// The systems that roll back a component no network replicates, which a predicted component
/// runs too.
pub(crate) fn add_non_networked_rollback_systems<C: Component<Mutability = Mutable> + Clone>(
    app: &mut App,
) {
    app.add_observer(PredictionHistory::<C>::record_removal);
    app.add_observer(PredictionHistory::<C>::add_on_prediction);
    // This also supports explicit resynchronization after prediction has begun. Do not shift
    // `ConfirmedHistory<C>` here: confirmed samples are resolved
    // through `ConfirmedTickMap` and are already in authoritative
    // server tick space. Shifting them can move authoritative state into the
    // future and make rollback prefer it over later server updates.
    app.add_observer(PredictionHistory::<C>::shift_with_local_timeline);
    app.add_systems(
        FixedPostUpdate,
        // we need to run this during fixed update to know accurately the history for each tick
        PredictionHistory::<C>::update.in_set(PredictionSystems::UpdateHistory),
    );
    app.add_systems(
        PostUpdate,
        PredictionHistory::<C>::prune_predicted.in_set(PredictionSystems::All),
    );
    app.add_systems(
        PreUpdate,
        RollbackPlugin::prepare_rollback::<C>.in_set(RollbackSystems::Prepare),
    );
}

/// The systems that predict a replicated component: its local rollback's, and the confirmed
/// history's that it checks the prediction against.
pub(crate) fn add_prediction_systems<C: HistoryComponent>(app: &mut App) {
    add_non_networked_rollback_systems::<C>(app);
    app.add_observer(PredictionHistory::<C>::backfill_confirmed);
    // Prune once per frame in PostUpdate (not before the rollback check): Check must
    // see the full confirmed history for this frame's rollback decision, including
    // snapshots received in PreUpdate. PostUpdate runs after the fixed loop,
    // so this still bounds growth before the next frame's Check, without running
    // inside rollback replay.
    app.add_systems(
        PostUpdate,
        PredictionHistory::<C>::prune_confirmed
            .in_set(PredictionSystems::All)
            .in_set(ReplicationSystems::CutHistory),
    );
    app.add_systems(
        PreUpdate,
        (
            RollbackScan::survey_check_targets::<C>.in_set(RollbackCheckSystems::Survey),
            RollbackScan::scan_mismatches::<C>.in_set(RollbackCheckSystems::Scan),
        ),
    );
    app.add_systems(
        FixedPreUpdate,
        PredictionHistory::<C>::snap_to_confirmed.in_set(PredictionSystems::SnapToConfirmed),
    );
}

pub(crate) fn add_prediction_diff_systems<C: HistoryComponent + RepliconDiffable>(app: &mut App) {
    app.add_observer(PredictionHistory::<C>::add_diff_receiver);
    app.add_systems(
        PreUpdate,
        RollbackScan::survey_pending_diffs::<C>.in_set(RollbackCheckSystems::Survey),
    );
}

impl Plugin for PredictionPlugin {
    fn finish(&self, app: &mut App) {
        prediction_registry::install(app);
    }

    fn build(&self, app: &mut App) {
        // A rollback hides what it disables, and a predicted despawn what it despawned, from every
        // query, as Bevy's own disabled entities are.
        let rollback_disable_id = app
            .world_mut()
            .register_component::<DisabledDuringRollback>();
        let prediction_despawned_id = app.world_mut().register_component::<PredictionDespawned>();
        app.world_mut()
            .resource_mut::<DefaultQueryFilters>()
            .register_disabling_component(rollback_disable_id);
        app.world_mut()
            .resource_mut::<DefaultQueryFilters>()
            .register_disabling_component(prediction_despawned_id);

        // The rollback check reads the confirmed values this frame received.
        app.configure_sets(
            PreUpdate,
            (
                ReplicationSystems::Receive,
                PredictionSystems::Rollback.in_set(PredictionSystems::All),
            )
                .chain(),
        );
        app.configure_sets(
            PreUpdate,
            PredictionSystems::All.run_if(PredictionSetup::should_run),
        );

        // A replayed tick starts from the confirmed values where they are known.
        app.configure_sets(
            FixedPreUpdate,
            PredictionSystems::SnapToConfirmed
                .in_set(PredictionSystems::All)
                .run_if(Rollback::is_active),
        );
        app.configure_sets(
            FixedPreUpdate,
            PredictionSystems::All.run_if(PredictionSetup::should_run),
        );

        app.configure_sets(
            FixedPostUpdate,
            (
                PredictionSystems::EntityDespawn,
                // for pre-spawned entities that could be spawned during FixedUpdate, we want to add
                // the history right away to avoid rollbacks
                PredictionSystems::UpdateHistory,
            )
                .in_set(PredictionSystems::All)
                .chain(),
        );
        app.configure_sets(
            FixedPostUpdate,
            PredictionSystems::All.run_if(PredictionSetup::should_run),
        );
        app.add_systems(
            FixedPostUpdate,
            (
                Self::finalize_local_despawns,
                Self::forget_unreachable_pre_spawns,
            )
                .in_set(PredictionSystems::EntityDespawn),
        );

        app.configure_sets(
            PostUpdate,
            PredictionSystems::All.run_if(PredictionSetup::should_run),
        );
    }
}

impl PredictionPlugin {
    /// Permanently remove locally predicted entities once their despawn tick is outside the
    /// rollback window.
    pub(crate) fn finalize_local_despawns(
        mut commands: Commands<'_, '_>,
        timeline: Res<'_, LocalTimeline>,
        prediction_setup: Res<'_, PredictionSetup>,
        input_setup: Res<'_, InputTimelineSetup>,
        query: Query<
            '_,
            '_,
            (Entity, &PredictionDespawned),
            (With<LocallyPredicted>, Allow<PredictionDespawned>),
        >,
    ) {
        let oldest = prediction_setup.oldest_rollback_tick(timeline.tick(), &input_setup);
        for (entity, disabled) in &query {
            // Keep the entity while its disable tick can still be reached by an accepted rollback.
            // Once it is older than the same effective bound enforced by
            // `RollbackPlugin::check_rollback`, no later reconciliation can restore it.
            if disabled.tick < oldest {
                commands
                    .entity(entity)
                    .queue_handled(DespawnReplicated, error::warn);
            }
        }
    }

    /// Forgets the pre-spawned entities no rollback can reach, so the list a rollback walks holds
    /// only those its window covers.
    pub(crate) fn forget_unreachable_pre_spawns(
        mut receiver: ResMut<'_, PreSpawnedReceiver>,
        timeline: Res<'_, LocalTimeline>,
        prediction_setup: Res<'_, PredictionSetup>,
        input_setup: Res<'_, InputTimelineSetup>,
    ) {
        receiver.forget_rollback_through(
            prediction_setup.oldest_rollback_tick(timeline.tick(), &input_setup),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_net_connection::NetworkingState;

    use campfire_net_connection::NetworkTopology;

    use crate::fixtures::prediction_stack::PredictionStack;
    use campfire_net_core::NetTick;
    use campfire_net_replication::ComponentRegistry;
    use campfire_net_stepper::{Stepper, StepperSetup};

    use crate::prediction_plugins::PredictionPlugins;
    use crate::prediction_registry_plugin::PredictionRegistryPlugin;
    use bevy_ecs::system::RunSystemOnce;
    use campfire_net_frame_interpolation::FrameRegistryPlugin;

    #[derive(Debug, Resource, Default)]
    struct SnapRuns(u32);

    fn count_snap_runs(mut runs: ResMut<'_, SnapRuns>) {
        runs.0 += 1;
    }

    #[test]
    fn the_snap_to_the_confirmed_value_runs_only_in_a_rollback() {
        let mut app = App::new();
        // The resources the connection and replication layers provide.
        app.init_resource::<NetworkingState>();
        app.init_resource::<ComponentRegistry>();
        app.add_plugins((
            FrameRegistryPlugin,
            PredictionRegistryPlugin,
            PredictionPlugins,
        ));
        app.insert_resource(PredictionSetup::default());
        app.init_resource::<SnapRuns>();
        let client = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<NetworkingState>().topology =
            NetworkTopology::Client(client);
        app.add_systems(
            FixedPreUpdate,
            count_snap_runs.in_set(PredictionSystems::SnapToConfirmed),
        );

        app.world_mut().run_schedule(FixedPreUpdate);
        assert_eq!(app.world().resource::<SnapRuns>().0, 0);

        app.insert_resource(Rollback(NetTick(0)));
        app.world_mut().run_schedule(FixedPreUpdate);
        assert_eq!(app.world().resource::<SnapRuns>().0, 1);
    }

    /// A diff component's receiver is pruned after prediction cuts its history, so the receiver
    /// follows the front the cut leaves in the same frame.
    #[test]
    fn the_diff_receiver_prune_follows_the_history_cut() {
        let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
        stepper.frame_step(1);
        let app = stepper.client_app(0);
        let position = |system_type: &str, system_fn: &str| {
            app.world()
                .resource::<Schedules>()
                .get(PostUpdate)
                .unwrap()
                .systems()
                .unwrap()
                .position(|(_, system)| {
                    let name = system.name().to_string();
                    name.contains(system_type)
                        && name.ends_with(system_fn)
                        && name.contains("CompRepliconDiff")
                })
                .unwrap_or_else(|| panic!("{system_type}{system_fn} runs in PostUpdate"))
        };
        assert!(
            position("PredictionHistory<", ">::prune_confirmed")
                < position("HistoryDiffReceiver<", ">::prune")
        );
    }

    /// With a rollback window of `w` ticks at tick 100, an entity disabled at `100 - w - 1` can be
    /// restored by no rollback and is despawned, while one disabled at `100 - w`, the oldest tick a
    /// rollback restores, stays. A pre-spawn at `100 - w` is forgotten, as a rollback despawns only
    /// what spawned after the tick it restores, and one at `100 - w + 1` stays listed.
    #[test]
    fn a_local_despawn_ends_once_it_leaves_the_rollback_window() {
        let mut world = World::new();
        world.init_resource::<PredictionSetup>();
        world.init_resource::<InputTimelineSetup>();
        let mut timeline = LocalTimeline::default();
        timeline.shift(100);
        world.insert_resource(timeline);
        let window = u32::from(
            world
                .resource::<PredictionSetup>()
                .max_rollback_ticks(world.resource::<InputTimelineSetup>()),
        );
        let mut disabled_at = |tick| {
            world
                .spawn((LocallyPredicted::default(), PredictionDespawned { tick }))
                .id()
        };
        let gone = disabled_at(NetTick(100 - window - 1));
        let kept = disabled_at(NetTick(100 - window));

        world
            .run_system_once(PredictionPlugin::finalize_local_despawns)
            .unwrap();

        assert!(world.get_entity(gone).is_err());
        assert!(world.get_entity(kept).is_ok());

        let mut receiver = PreSpawnedReceiver::default();
        receiver.register(NetTick(100 - window), gone);
        receiver.register(NetTick(100 - window + 1), kept);
        world.insert_resource(receiver);
        world
            .run_system_once(PredictionPlugin::forget_unreachable_pre_spawns)
            .unwrap();
        assert_eq!(
            world.resource::<PreSpawnedReceiver>().rollback(),
            [(NetTick(100 - window + 1), kept)]
        );
    }
}
