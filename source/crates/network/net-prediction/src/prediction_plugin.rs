use crate::despawn::{finalize_local_despawns, forget_unreachable_pre_spawns};
use crate::disabled_during_rollback::DisabledDuringRollback;
use crate::misprediction::{scan_mismatches, survey_check_targets, survey_pending_diffs};
use crate::prediction_despawned::PredictionDespawned;
use crate::prediction_diagnostics_plugin::PredictionDiagnosticsPlugin;
use crate::prediction_gate;
use crate::prediction_history::{
    add_history_diff_receiver, add_prediction_history, apply_component_removal_predicted,
    backfill_confirmed_history_on_predicted, handle_local_timeline_shift_prediction_history,
    prune_confirmed_history, snap_to_confirmed, update_prediction_history,
};
use crate::prediction_registry;
use crate::prediction_registry_plugin::PredictionRegistryPlugin;
use crate::prediction_systems::PredictionSystems;
use crate::rollback_check_stage::RollbackCheckStage;
use crate::rollback_plugin::{RollbackPlugin, prepare_rollback};
use crate::rollback_systems::RollbackSystems;
use crate::switch;
use bevy_app::prelude::*;
use bevy_ecs::component::Mutable;
use bevy_ecs::entity_disabling::DefaultQueryFilters;
use bevy_ecs::prelude::*;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use campfire_net_interpolation::FrameInterpolationPlugin;
use campfire_net_interpolation::HistoryComponent;
use campfire_net_replication::ReplicationSystems;
use campfire_net_timeline::rollback;

/// Plugin that installs client-side prediction systems.
///
/// The systems run when the application contains a
/// [`PredictionManager`](crate::prediction_manager::PredictionManager) resource and its cached
/// network topology is a client. Prediction-history writers also require a synchronized timeline
/// and therefore never record values under pre-sync tick labels. Insert a `PredictionManager`
/// resource to opt the application into one global prediction and rollback pipeline. Host-client
/// and server topologies remain authoritative and do not run it.
#[derive(Debug, Default)]
pub struct PredictionPlugin;

// Higher-priority markers win when more than one applies to an update.
// Pre-spawn must preserve its specialized reconciliation semantics before the
// general prediction markers are considered. Pre-spawn is
// strongest because a locally predicted absence must not be materialized by a
// lower-priority initial-write path.
pub(crate) const PRE_SPAWNED_PRIORITY: usize = 120;
pub(crate) const PREDICTED_SEND_PRIORITY: usize = 100;
pub(crate) const PREDICTED_PRIORITY: usize = 90;

/// The systems that roll back a component no network replicates.
pub(crate) fn add_non_networked_rollback_systems<C: Component<Mutability = Mutable> + Clone>(
    app: &mut App,
) {
    app.add_observer(apply_component_removal_predicted::<C>);
    app.add_observer(add_prediction_history::<C>);
    // This also supports explicit resynchronization after prediction has begun. Do not shift
    // `ConfirmedHistory<C>` here: confirmed samples are resolved
    // through `ReplicationCheckpointMap` and are already in authoritative
    // server tick space. Shifting them can move authoritative state into the
    // future and make rollback prefer it over later server updates.
    app.add_observer(handle_local_timeline_shift_prediction_history::<C>);
    app.add_systems(
        FixedPostUpdate,
        update_prediction_history::<C>.in_set(PredictionSystems::UpdateHistory),
    );
    app.add_systems(
        PreUpdate,
        prepare_rollback::<C>.in_set(RollbackSystems::Prepare),
    );
}

pub(crate) fn add_prediction_systems<C: HistoryComponent>(app: &mut App) {
    app.add_observer(apply_component_removal_predicted::<C>);
    app.add_observer(handle_local_timeline_shift_prediction_history::<C>);
    app.add_observer(add_prediction_history::<C>);
    app.add_observer(backfill_confirmed_history_on_predicted::<C>);
    app.add_systems(
        FixedPostUpdate,
        // we need to run this during fixed update to know accurately the history for each tick
        update_prediction_history::<C>.in_set(PredictionSystems::UpdateHistory),
    );
    // Prune once per frame in PostUpdate (not before the rollback check): Check must
    // see the full confirmed history for this frame's rollback decision, including
    // snapshots received in PreUpdate. PostUpdate runs after the fixed loop,
    // so this still bounds growth before the next frame's Check, without running
    // inside rollback replay.
    app.add_systems(
        PostUpdate,
        prune_confirmed_history::<C>
            .in_set(PredictionSystems::All)
            .in_set(ReplicationSystems::CutHistory),
    );
    app.add_systems(
        PreUpdate,
        (
            survey_check_targets::<C>.in_set(RollbackCheckStage::Survey),
            scan_mismatches::<C>.in_set(RollbackCheckStage::Scan),
            prepare_rollback::<C>.in_set(RollbackSystems::Prepare),
        ),
    );
    app.add_systems(
        FixedPreUpdate,
        snap_to_confirmed::<C>.in_set(PredictionSystems::SnapToConfirmed),
    );
}

pub(crate) fn add_prediction_diff_systems<C: HistoryComponent + RepliconDiffable>(app: &mut App) {
    app.add_observer(add_history_diff_receiver::<C>);
    app.add_systems(
        PreUpdate,
        survey_pending_diffs::<C>.in_set(RollbackCheckStage::Survey),
    );
}

impl Plugin for PredictionPlugin {
    fn finish(&self, app: &mut App) {
        prediction_registry::install(app);
    }

    fn build(&self, app: &mut App) {
        // Correction measures each error against the frame-interpolated value.
        if !app.is_plugin_added::<FrameInterpolationPlugin>() {
            app.add_plugins(FrameInterpolationPlugin);
        }
        if !app.is_plugin_added::<PredictionRegistryPlugin>() {
            app.add_plugins(PredictionRegistryPlugin);
        }

        // Custom entity disabling
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

        // PreUpdate systems:
        // 1. Receive confirmed entities, add Confirmed and Predicted components
        // 2. (in prediction_systems) add ComponentHistory
        // 3. (in prediction_systems) Check if we should do rollback, clear histories and snap prediction's history to server-state
        // 4. Potentially do rollback
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
            PredictionSystems::All.run_if(prediction_gate::should_run),
        );

        // FixedPreUpdate systems
        // - During rollback, snap components to confirmed values if we have them
        app.configure_sets(
            FixedPreUpdate,
            PredictionSystems::SnapToConfirmed
                .in_set(PredictionSystems::All)
                .run_if(rollback::is_active),
        );
        app.configure_sets(
            FixedPreUpdate,
            PredictionSystems::All.run_if(prediction_gate::should_run),
        );

        // FixedPostUpdate systems
        // 1. Update client tick (don't run in rollback)
        // 2. Run main physics/game fixed-update loop
        // 3. Increment rollback tick (only run in fallback)
        // 4. Update predicted history
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
            PredictionSystems::All.run_if(prediction_gate::should_run),
        );
        app.add_systems(
            FixedPostUpdate,
            (finalize_local_despawns, forget_unreachable_pre_spawns)
                .in_set(PredictionSystems::EntityDespawn),
        );

        app.configure_sets(
            PostUpdate,
            PredictionSystems::All.run_if(prediction_gate::should_run),
        );

        // Timeline switch between predicted and interpolated (client-local).
        switch::add_timeline_switch_systems(app);

        app.add_plugins((PredictionDiagnosticsPlugin::default(), RollbackPlugin));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prediction_manager::PredictionManager;
    use campfire_net_connection::NetworkingState;

    use campfire_net_connection::NetworkTopology;

    use crate::internals::prediction_stack::PredictionStack;
    use campfire_net_core::NetTick;
    use campfire_net_replication::ComponentRegistry;
    use campfire_net_stepper::{Stepper, StepperConfig};
    use campfire_net_timeline::rollback::Rollback;

    #[derive(Debug, Resource, Default)]
    struct SnapRuns(u32);

    fn count_snap_runs(mut runs: ResMut<'_, SnapRuns>) {
        runs.0 += 1;
    }

    #[test]
    fn snap_to_confirmed_set_only_runs_during_rollback() {
        let mut app = App::new();
        // The resources the connection and replication layers provide.
        app.init_resource::<NetworkingState>();
        app.init_resource::<ComponentRegistry>();
        app.add_plugins(PredictionPlugin);
        app.insert_resource(PredictionManager::default());
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
        let mut stepper = Stepper::new(PredictionStack, StepperConfig::single());
        stepper.frame_step(1);
        let app = stepper.client_app(0);
        let position = |prefix: &str| {
            app.world()
                .resource::<Schedules>()
                .get(PostUpdate)
                .unwrap()
                .systems()
                .unwrap()
                .position(|(_, system)| {
                    let name = system.name().to_string();
                    name.contains(prefix) && name.contains("CompRepliconDiff")
                })
                .unwrap_or_else(|| panic!("{prefix} runs in PostUpdate"))
        };
        assert!(position("prune_confirmed_history<") < position("prune_history_diff_receiver<"));
    }
}
