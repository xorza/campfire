/*!
Rollback idea:

 Key insight: the latest completed mutate tick T guarantees that for entities not updated at tick T,
their value is equal to the last confirmed value.

Proof:
Let's say the latest completed mutate tick is T, and we only received a message for entity A. (there is another entity B).
Does that mean that we fully know the state of entity B? How do we determine the confirmed value for B? We know that the value of B did not change on tick T-1.
- either we received an update for B on tick T-1, then we know that at tick T the value of B is the same
- either we know mutate tick T-1 is complete, then we know that B at tick T-1 is the same as the previous confirmed value
- either we don't know mutate tick T-1 is complete. We could have:
  - the server did not send any message with an update to B, so B is the same as the previous confirmed value
  - the server sent a message with an update for B, but the message is lost or in-flight. But in that case the server would not have received an ack for that message, so on tick T it would have sent an update for B again! So that is not possible.
    That means that we know for sure that B did not change compared to its last confirmed value.

Then the question becomes, how does that affect how we rollback?
We need:
- when we receive an update, we add its authoritative value to confirmed history;
- once a completed checkpoint advances, we compare every replicated predicted component at that
  checkpoint. We cannot skip an entity just because it received an update: confirmation is tracked
  per entity, and another predicted component on that entity may have been unchanged and mismatched;
- To rollback we have 2 choices:
  - rollback from the earliest confirmed tick across all predicted entities (predicted entities are a subset of all entities so it's possible that this is more recent than the latest completed mutate tick)
  - rollback from the latest completed mutate tick
    For simplicity we will do the second choice
- One thing to be careful of is that we could have completed mutate tick T, but have received confirmed updates for ticks > T. In which case we don't want to overwrite them when we rollback, and instead use these confirmed values during the rollback!

The replay rewinds `Time<Fixed>` and `Time<()>`, which the stack owns. Any other clock that
`FixedMain` advances, as a physics engine's, rolls back as other state does:
`register_component::<Time<C>>().local_rollback()` keeps its history, and its writer runs before
[`PredictionSystems::UpdateHistory`](crate::prediction_systems::PredictionSystems), so the history
holds the value of each tick.
 */

use campfire_net_despawn::DespawnReplicated;

use crate::disable_rollback::DisableRollback;
use crate::disabled_during_rollback::DisabledDuringRollback;
use crate::locally_predicted::LocallyPredicted;
use crate::prediction_counts::PredictionCounts;
use crate::prediction_despawned::PredictionDespawned;
use crate::prediction_history::PredictionHistory;
use crate::prediction_manager::PredictionManager;
use crate::prediction_registry::PredictionRegistry;
use crate::prediction_systems::PredictionSystems;
use crate::previous_visual::PreviousVisual;
use crate::rollback_check_stage::RollbackCheckStage;
use crate::rollback_check_state::{RollbackCheckState, RollbackKind};
use crate::rollback_mode::RollbackMode;
use crate::rollback_scan::RollbackScan;
use crate::rollback_systems::RollbackSystems;
use crate::tick_period_history::TickPeriodHistory;
use bevy_app::FixedMain;
use bevy_app::prelude::*;
use bevy_ecs::component::Mutable;
use bevy_ecs::entity::EntityHashSet;
use bevy_ecs::prelude::*;
use bevy_replicon::client::server_mutate_ticks::ServerMutateTicks;
use bevy_replicon::prelude::{ClientMessages, ClientSystems, Remote};
use bevy_replicon::shared::backend::channels::ServerChannel;
use bevy_time::{Fixed, Time};
use bevy_utils::prelude::DebugName;
use campfire_net_connection::NetworkingState;
use campfire_net_core::NetTick;
use campfire_net_core::TickRate;
use campfire_net_interpolation::FrameInterpolationSystems;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::DeferredEntityCommands;
use campfire_net_replication::HistoryState;
use campfire_net_replication::Interpolated;
use campfire_net_replication::PreSpawned;
use campfire_net_replication::PreSpawnedReceiver;
use campfire_net_replication::Predicted;
use campfire_net_replication::ReplicationCheckpointMap;
use campfire_net_replication::ReplicationSystems;
use campfire_net_sync::{InputTimelineConfig, SyncedLocalTimeline};
use campfire_net_timeline::LocalTimeline;
use campfire_net_timeline::Rollback;
use core::fmt::Debug;
use tracing::{debug, debug_span, trace};

/// Installs prediction rollback detection, restoration, and replay.
#[derive(Debug)]
pub struct RollbackPlugin;

impl Plugin for RollbackPlugin {
    fn build(&self, app: &mut App) {
        // The other layers' resources come from their plugins: the connection's topology, and
        // replication's registries, checkpoints and pre-spawn receiver.
        app.init_resource::<RollbackCheckState>();
        app.init_resource::<RollbackScan>();
        app.init_resource::<TickPeriodHistory>();
        app.add_observer(TickPeriodHistory::shift_with_local_timeline);
        app.add_systems(FixedPostUpdate, TickPeriodHistory::record_tick);

        app.configure_sets(
            PreUpdate,
            (
                RollbackSystems::Check,
                RollbackSystems::RestoreDespawned.run_if(Rollback::is_active),
                RollbackSystems::Prepare.run_if(Rollback::is_active),
                RollbackSystems::Rollback.run_if(Rollback::is_active),
                RollbackSystems::EndRollback.run_if(Rollback::is_active),
            )
                .chain()
                .in_set(PredictionSystems::Rollback),
        );
        app.configure_sets(
            PreUpdate,
            (
                RollbackCheckStage::Begin,
                RollbackCheckStage::Survey,
                RollbackCheckStage::Decide,
                RollbackCheckStage::Scan,
                RollbackCheckStage::Finish,
            )
                .chain()
                .in_set(RollbackSystems::Check),
        );
        app.configure_sets(
            PreUpdate,
            RollbackSystems::Check.after(ReplicationSystems::Receive),
        );
        app.configure_sets(
            PostUpdate,
            // we add the correction error AFTER the interpolation was done
            // (which means it's also after we buffer the component for replication)
            RollbackSystems::VisualCorrection
                .after(FrameInterpolationSystems::Interpolate)
                .in_set(PredictionSystems::All),
        );

        app.add_systems(
            PreUpdate,
            (
                Self::reset_rollback_check_state_on_topology_change
                    .before(ReplicationSystems::Receive)
                    .before(RollbackSystems::Check),
                Self::check_received_replication_messages
                    .after(ClientSystems::ReceivePackets)
                    .before(ClientSystems::Receive),
                Self::begin_rollback_check.in_set(RollbackCheckStage::Begin),
                Self::check_rollback.in_set(RollbackCheckStage::Decide),
                Self::finish_rollback_check.in_set(RollbackCheckStage::Finish),
                (Self::start_rollback, Self::remove_prediction_despawned)
                    .chain()
                    .in_set(RollbackSystems::RestoreDespawned),
                Self::run_rollback.in_set(RollbackSystems::Rollback),
                Self::end_rollback.in_set(RollbackSystems::EndRollback),
                Self::prune_local_despawn
                    .after(RollbackSystems::EndRollback)
                    .in_set(PredictionSystems::Rollback),
            ),
        );
    }
}

impl RollbackPlugin {
    /// Before we start preparing for rollback, restore each predicted entity whose despawn the
    /// rollback undoes: one the server holds, as its authority wins, and one a prediction despawned
    /// after the rollback tick, as the replay predicts its despawn again. A locally predicted entity
    /// despawned at or before the tick was already gone there, and stays disabled.
    pub(crate) fn remove_prediction_despawned(
        mut commands: Commands<'_, '_>,
        rollback: Res<'_, Rollback>,
        query: Query<
            '_,
            '_,
            (Entity, &PredictionDespawned, Has<Remote>),
            (
                Allow<PredictionDespawned>,
                Or<(With<Predicted>, With<LocallyPredicted>, With<PreSpawned>)>,
            ),
        >,
    ) {
        for (entity, disable, server_held) in &query {
            if server_held || disable.tick > rollback.0 {
                trace!(
                    ?entity,
                    "Removing PredictionDespawned marker before rollback preparation"
                );
                commands.entity(entity).try_remove::<PredictionDespawned>();
            }
        }
    }

    /// If there is a mismatch, prepares `C` for the rollback, on each entity that keeps its history
    /// and is not kept out of rollbacks.
    ///
    /// This function:
    /// 1. Clears all **predicted** values from rollback_tick onwards (we will re-predict them)
    /// 2. Preserves all **confirmed** values (we know the real server values and will snap to them during re-simulation)
    /// 3. Reverts the component to the value at rollback_tick
    ///
    /// Neither history is marked changed, as their types define.
    pub(crate) fn prepare_rollback<C: Component<Mutability = Mutable> + Clone>(
        timeline: Res<'_, LocalTimeline>,
        rollback: Res<'_, Rollback>,
        registry: Res<'_, PredictionRegistry>,
        mut query: Query<
            '_,
            '_,
            (
                Entity,
                Option<&mut C>,
                Option<&ConfirmedHistory<C>>,
                &mut PredictionHistory<C>,
                Has<Interpolated>,
            ),
            Without<DisableRollback>,
        >,
        mut deferred: Deferred<'_, DeferredEntityCommands>,
    ) {
        let current_tick = timeline.tick();
        let rollback_tick = rollback.0;
        let kind = DebugName::type_name::<C>();
        // Interpolated visuals are presentation-owned: rollback must still rewind these components
        // for replay consistency, but it must not capture them for live-value correction, which has
        // no rule coverage for interpolation-driven archetypes.
        let corrected = registry.corrects::<C>();
        for (entity_id, mut live, confirmed_history, mut predicted_history, interpolated) in
            &mut query
        {
            let predicted_history = predicted_history.bypass_change_detection();
            let restore_state = if let Some(confirmed_history) = confirmed_history {
                // A policy-driven state rollback uses the diff-aware completed checkpoint C stored
                // in PredictionManager as its rollback target. Completion of C proves that a
                // component without an explicit update is unchanged since its previous confirmed
                // state, even if the check wrote no anchor at C because it stopped at the first
                // mismatch.
                //
                // A forced state rollback uses the same lookup, but relies on the caller's stronger
                // precondition: its explicit rollback target has already been seeded with
                // authoritative confirmed history.
                confirmed_history
                    .get_state_at_or_before(rollback_tick)
                    .cloned()
            } else {
                // State rollback can also cover predicted-only local components
                // that have no authoritative history yet.
                predicted_history
                    .get_state_at_or_before(rollback_tick)
                    .cloned()
            };
            // Keep the prediction history anchored at the actual rollback target.
            // For completed state rollbacks this is the completed mutate tick; for
            // forced rollbacks it may be older than the latest completed mutate tick.
            //
            // Discard only the entries the rollback invalidates (strictly newer
            // than the target): the replay rewrites those from the restored state.
            // Entries at-or-before the target are still the client's canonical
            // past — the replay never touches them — and they are exactly what a
            // later, DEEPER rollback in the same frame window needs as its floor
            // sample. Clearing the whole buffer here would throw them away, so a
            // second rollback to an older tick would find no sample at-or-before
            // its target and fall through to seeding the history with the
            // current-tick live value — injecting a future value into the past
            // (permanent divergence, see the Some(stale) class in issue #1511).
            predicted_history.clear_after_tick(rollback_tick);
            if let Some(state) = restore_state.clone() {
                predicted_history.add_state(rollback_tick, state);
            } else if let Some(live) = &live {
                // No state exists at rollback_tick (e.g. the entity was revealed to
                // this client after the rollback target). Prefer the oldest present
                // confirmed sample when one exists: it is the nearest known data
                // to the target (its interval to any tick in the youth gap is a
                // subset of the live value's), so a later restore that floors onto
                // this seed starts from real data instead of a future value. The
                // live component is still left in place either way, and replay
                // snap-corrects onto confirmed samples tick by tick as it reaches
                // them. Without any confirmed data, the replay starts from the
                // current component value, so seed the history with it.
                let seed = confirmed_history
                    .and_then(ConfirmedHistory::start_present)
                    .map_or_else(|| (**live).clone(), |entry| entry.value.clone());
                predicted_history.add_state(rollback_tick, HistoryState::Updated(seed));
            }
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "prepare_rollback_component",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                entity = ?entity_id,
                component = ?kind,
                local_tick = current_tick.0,
                rollback_tick = rollback_tick.0,
                history_len = predicted_history.len(),
                "prepared component rollback"
            );

            // Keep track of the value on screen so the correction can smooth the jump
            // from it, whatever the rollback is about to do to the component: it may
            // leave the value alone, remove it, or replace it. The component may also
            // be removed now and re-added later in the replay, and the saved value is
            // what a visual correction is measured against when it comes back.
            if corrected
                && !interpolated
                && let Some(predicted_component) = live.as_deref().cloned()
            {
                // The slot came with the history, so the write moves no table.
                deferred.insert(entity_id, PreviousVisual(Some(predicted_component)));
                trace!(
                    target: "campfire_net_debug::prediction",
                    kind = "previous_visual_stored",
                    schedule = "PreUpdate",
                    sample_point = "PreUpdate",
                    entity = ?entity_id,
                    component = ?kind,
                    local_tick = current_tick.0,
                    rollback_tick = rollback_tick.0,
                    "stored previous visual for correction"
                );
            }

            match restore_state {
                // No state exists at rollback_tick. This is not an explicit
                // removal, so leave the current component value in place.
                None => {
                    trace!(
                        entity = ?entity_id,
                        ?kind,
                        ?rollback_tick,
                        "No history entry for component at rollback tick; leaving current value in place"
                    );
                }
                // An explicit removal means the component was authoritatively removed at rollback_tick.
                Some(HistoryState::Removed) => {
                    if live.is_some() {
                        deferred.remove::<C>(entity_id);
                    }
                    trace!("Removing component from predicted entity for rollback");
                }
                Some(HistoryState::Updated(correct)) => {
                    if let Some(live) = &mut live {
                        **live = correct;
                    } else {
                        debug!("Re-adding deleted component to predicted");
                        deferred.insert(entity_id, correct);
                    }
                }
            }
        }
    }

    pub(crate) fn run_rollback(
        world: &mut World,
        disable_rollback: &mut QueryState<Entity, With<DisableRollback>>,
        mut disabled_entities: Local<'_, Vec<Entity>>,
    ) {
        let local_timeline = world.resource_mut::<LocalTimeline>();
        let current_tick = local_timeline.tick();
        let rollback_start_tick = world.resource::<Rollback>().0;

        // The components hold their state at the end of `rollback_start_tick`, so the timeline goes
        // back there and the replay runs forward to the end of `current_tick`.
        let num_rollback_ticks = current_tick - rollback_start_tick;
        world
            .resource_mut::<LocalTimeline>()
            .shift(-num_rollback_ticks);
        debug!(
            "Rollback between {:?} and {:?}",
            rollback_start_tick, current_tick
        );
        trace!(
            target: "campfire_net_debug::prediction",
            kind = "rollback_start",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            local_tick = current_tick.0,
            rollback_tick = rollback_start_tick.0,
            num_rollback_ticks,
            "starting rollback"
        );

        let time_resource = *world.resource::<Time>();

        let current_fixed_time = *world.resource::<Time<Fixed>>();
        let current_period = current_fixed_time.timestep();
        *world.resource_mut::<Time<Fixed>>() = world.resource::<TickPeriodHistory>().rewind(
            &current_fixed_time,
            rollback_start_tick,
            current_tick,
        );

        // Bevy disables an entity only while it holds a disabling component, so the replay inserts
        // and removes one: a slot that held its state would leave the entity in every query.
        disabled_entities.clear();
        disabled_entities.extend(disable_rollback.iter(world));
        for &entity in &*disabled_entities {
            world.entity_mut(entity).insert(DisabledDuringRollback);
        }

        // Run the fixed update schedule (which should contain ALL
        // predicted/rollback components and resources). This is similar to what
        // `bevy_time::fixed::run_fixed_main_schedule()` does
        for i in 0..num_rollback_ticks {
            // we add 1 here because running FixedUpdate will start by incrementing the tick
            let rollback_tick = rollback_start_tick + i + 1;
            let _span = debug_span!("rollback", tick = ?rollback_tick).entered();
            debug!(?rollback_tick, "rollback");
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "rollback_tick",
                schedule = "FixedMain",
                sample_point = "FixedUpdate",
                local_tick = rollback_tick.0,
                rollback_tick = rollback_tick.0,
                rollback_iteration = i,
                num_rollback_ticks,
                "running rollback tick"
            );
            // A tick that did not run, as one a resync skipped, runs now at the current period.
            let period = world
                .resource::<TickPeriodHistory>()
                .period(rollback_tick)
                .unwrap_or(current_period);
            let mut fixed_time = world.resource_mut::<Time<Fixed>>();
            fixed_time.set_timestep(period);
            fixed_time.advance_by(period);
            *world.resource_mut::<Time>() = world.resource::<Time<Fixed>>().as_generic();

            world.run_schedule(FixedMain);
        }

        // The replay can despawn an entity.
        for &entity in &*disabled_entities {
            if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                entity_mut.remove::<DisabledDuringRollback>();
            }
        }

        // `current_fixed_time` and the fixed time resource in use (e.g. the
        // rollback fixed time) should be the same after the rollback except that
        // `current_fixed_time` may have an overstep. Use `current_fixed_time` so
        // its overstep isn't lost.
        *world.resource_mut::<Time<Fixed>>() = current_fixed_time;
        // A replayed `SetTickRate` changed the rate after the restored timestep was read.
        let period = world.resource::<TickRate>().period();
        world.resource_mut::<Time<Fixed>>().set_timestep(period);

        *world.resource_mut::<Time>() = time_resource;
        debug!("Finished rollback. Current tick: {:?}", current_tick);
        trace!(
            target: "campfire_net_debug::prediction",
            kind = "rollback_finish",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            local_tick = current_tick.0,
            rollback_tick = rollback_start_tick.0,
            num_rollback_ticks,
            "finished rollback"
        );

        let mut counts = world.get_resource_mut::<PredictionCounts>().unwrap();
        counts.rollbacks = counts.rollbacks.saturating_add(1);
        // A start tick after the current tick replays no tick.
        let replayed = num_rollback_ticks.max(0).unsigned_abs();
        counts.rollback_ticks = counts.rollback_ticks.saturating_add(replayed);
    }

    pub(crate) fn end_rollback(
        rollback: Res<'_, Rollback>,
        mut check_state: ResMut<'_, RollbackCheckState>,
        mut commands: Commands<'_, '_>,
    ) {
        check_state.rollback_kind = RollbackKind::Policy;
        trace!(
            target: "campfire_net_debug::prediction",
            kind = "rollback_end",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            rollback_tick = rollback.0.0,
            "ending rollback"
        );
        commands.remove_resource::<Rollback>();
    }

    /// Set a flag if we received any replication message this frame.
    /// Also reset the per-frame state.
    fn check_received_replication_messages(
        client_messages: Option<Res<'_, ClientMessages>>,
        mut check_state: ResMut<'_, RollbackCheckState>,
    ) {
        check_state.reset_frame_state();

        if client_messages.is_some_and(|messages| {
            messages.received_count(ServerChannel::Updates) > 0
                || messages.received_count(ServerChannel::Mutations) > 0
        }) {
            check_state.received_messages_this_frame = true;
        }
    }

    fn reset_rollback_check_state_on_topology_change(
        networking: Res<'_, NetworkingState>,
        mut check_state: ResMut<'_, RollbackCheckState>,
    ) {
        if networking.is_changed() {
            check_state.reset_connection_state();
        }
    }

    /// Check if we need to do a rollback. We do this separately from `RollbackPlugin::prepare_rollback` because even if
    /// we stop the `RollbackPlugin::check_rollback` function early as soon as we find a mismatch, but we need to
    /// rollback all components to the original state.
    ///
    /// Key invariant: a completed checkpoint C guarantees that for all entities, we have complete
    /// information at C:
    /// - entities that received an update at C have their confirmed value in history;
    /// - entities that did not receive an update have the same value as their last confirmation before C.
    #[expect(
        clippy::too_many_arguments,
        reason = "a Bevy system takes each resource it reads"
    )]
    fn check_rollback(
        timeline: SyncedLocalTimeline<'_, '_>,
        input_config: Res<'_, InputTimelineConfig>,
        prediction_manager: Res<'_, PredictionManager>,
        mut check_state: ResMut<'_, RollbackCheckState>,
        checkpoints: Res<'_, ReplicationCheckpointMap>,
        server_mutate_ticks: Res<'_, ServerMutateTicks>,
        server_held_disabled: Query<
            '_,
            '_,
            (Entity, &PredictionDespawned),
            (With<Predicted>, With<Remote>, Allow<PredictionDespawned>),
        >,
        mut scan: ResMut<'_, RollbackScan>,
    ) {
        let tick = timeline.tick();
        let received_state = check_state.received_messages_this_frame;
        let max_rollback_ticks = prediction_manager.max_rollback_ticks(&input_config);
        let server_confirmed_tick = checkpoints.last_confirmed_tick();
        let candidate_confirmed_tick = scan.candidate;

        // Forced rollbacks (requested externally via
        // `RollbackCheckState::request_forced_rollback`) take precedence over
        // policy-driven rollbacks and fire regardless of `rollback_config.mode`.
        // This lets one-shot mechanisms (e.g. a late-join snapshot) trigger a
        // rollback even on a client whose normal rollback policy is `Disabled`.
        let forced_rollback_kind = core::mem::take(&mut check_state.forced_rollback_kind);
        if let Some(forced_tick) = check_state.forced_rollback_tick.take() {
            debug!(
                ?forced_tick,
                ?forced_rollback_kind,
                "Forced rollback requested via RollbackCheckState::request_forced_rollback"
            );
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "forced_rollback",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                local_tick = tick.0,
                rollback_tick = forced_tick.0,
                "forced rollback requested"
            );
            scan.request(forced_tick, max_rollback_ticks, forced_rollback_kind);
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "policy_rollbacks_skipped_forced",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                local_tick = tick.0,
                "policy-driven rollback checks skipped because a forced rollback is already requested"
            );
            return;
        }

        // Start from the newest globally completed checkpoint at or before the local tick, then
        // constrain it by unresolved diffs on predicted diff-replicated components.
        //
        // We might be in a situation where a tick is confirmed in ServerMutateTicks, but the diff
        // is not ready and is instead in 'pending' state. The surveys found the earliest 'pending'
        // tick before our candidate. If there are none, we can use C0. Else we find the earliest
        // confirmed tick before that.
        let earliest_pending_tick = scan.earliest_pending;
        let confirmed_tick = |candidate: NetTick| {
            let confirmed = match earliest_pending_tick {
                // P itself is unresolved, so the bound is strict. The selected rollback target is
                // still required to be a globally completed Replicon checkpoint.
                Some(pending_tick) => checkpoints
                    .latest_completed_before(&server_mutate_ticks, pending_tick)
                    .map(|checkpoint| checkpoint.tick),
                None => Some(candidate),
            };
            debug_assert!(confirmed.is_none_or(|confirmed| confirmed <= candidate));
            confirmed
        };

        // If we check for rollback on both state and input, state takes precedence.
        match prediction_manager.rollback_config.mode {
            // if we received a state update, we don't check for mismatches and just set the rollback
            // tick
            RollbackMode::Always => {
                if received_state
                    && scan.has_check_entities
                    && let Some(candidate) = candidate_confirmed_tick
                    && let Some(confirmed_tick) = confirmed_tick(candidate)
                {
                    debug!(
                        ?confirmed_tick,
                        "Rollback because we have received a new confirmed state. (no mismatch check)"
                    );
                    scan.request(confirmed_tick, max_rollback_ticks, RollbackKind::Policy);
                    check_state.set_last_processed_confirmed_tick(confirmed_tick);
                }
            }
            RollbackMode::Check => {
                // Do a rollback check using the confirmed state at C (which is guaranteed to
                // be complete) and the predicted history state at C.
                // The pending-diff bound only lowers the candidate, so a candidate the check
                // already handled skips the scan for it.
                if let Some(candidate) = candidate_confirmed_tick
                    && check_state.has_confirmed_tick_advanced(candidate)
                    && let Some(confirmed_tick) = confirmed_tick(candidate)
                    && check_state.has_confirmed_tick_advanced(confirmed_tick)
                {
                    // A completed mutate tick C certifies every replicated component at C, so
                    // always scan all of them. Explicitly updated components use their received
                    // samples, while unchanged components use their last authoritative value
                    // because completion proves it carried forward to C.
                    trace!(
                        ?tick,
                        ?confirmed_tick,
                        latest_completed_tick = ?server_confirmed_tick,
                        "Checking for state-based rollback at completed mutate tick"
                    );

                    // A despawn predicted at or before C of an entity the server still holds
                    // at C is a misprediction: the rollback brings the entity back.
                    if let Some(entity) =
                        server_held_disabled.iter().find_map(|(entity, disable)| {
                            (disable.tick <= confirmed_tick).then_some(entity)
                        })
                    {
                        debug!(
                            ?confirmed_tick,
                            ?entity,
                            "Rollback because a predicted despawn of a server-held entity is at or before the completed checkpoint"
                        );
                        scan.request(confirmed_tick, max_rollback_ticks, RollbackKind::Policy);
                    } else {
                        // Each predicted component compares its predicted value at C with its
                        // effective authoritative value, in the scan stage.
                        scan.scan_tick = Some(confirmed_tick);
                    }

                    // The scan of C ends this stage of the frame's check, so C is processed.
                    check_state.set_last_processed_confirmed_tick(confirmed_tick);
                }
            }
            RollbackMode::Disabled => {}
        }
    }

    /// Selects the newest completed checkpoint at or before the local tick, and whether this
    /// frame's policy looks at it, for the components' surveys. It resets the frame's scan first, so
    /// a frame before the timeline syncs asks for no rollback.
    fn begin_rollback_check(
        timeline: Option<SyncedLocalTimeline<'_, '_>>,
        prediction_manager: Res<'_, PredictionManager>,
        check_state: Res<'_, RollbackCheckState>,
        checkpoints: Res<'_, ReplicationCheckpointMap>,
        server_mutate_ticks: Res<'_, ServerMutateTicks>,
        mut scan: ResMut<'_, RollbackScan>,
    ) {
        *scan = RollbackScan::default();
        let Some(timeline) = timeline else {
            return;
        };
        let tick = timeline.tick();
        scan.tick = Some(tick);
        // Let:
        // - T be the current local simulation tick;
        // - S be the globally latest confirmed checkpoint;
        // - C0 be the latest confirmed checkpoint with C0 <= T;
        // - P be the earliest unresolved diff tick at or before C0;
        // - C be C0 when no such P exists, otherwise the latest completed checkpoint before P.
        //
        // In the usual case, the client is ahead of the server so S <= T and C0 = S. The
        // completed-checkpoint scan checks all replicated predicted components. This is
        // required for correctness: Replicon's confirmation marker is
        // entity-level, so a receive-time check would skip an updated entity even when
        // another predicted component on that entity was unchanged and mismatched.
        //
        // If S > T, Replicon's completion history lets us find C0 even though the latest completed
        // checkpoint is still in the local future. Diff replication adds one more constraint: an
        // unresolved diff at P makes the effective authoritative state unknown at every tick from P
        // onward. We therefore cannot use C0 when P <= C0. In that case, choose the latest globally
        // completed checkpoint strictly before the earliest such P. We intentionally derive that
        // fallback from ServerMutateTicks rather than ConfirmedHistory: histories contain explicit
        // component updates and selected unchanged anchors, not every globally completed checkpoint.
        let candidate = match checkpoints.last_confirmed_tick() {
            Some(confirmed_tick) if confirmed_tick <= tick => Some(confirmed_tick),
            Some(_) => checkpoints
                .latest_completed_at_or_before(&server_mutate_ticks, tick)
                .map(|checkpoint| checkpoint.tick),
            None => None,
        };
        scan.candidate = candidate;
        // A forced rollback skips the policy, which then looks at nothing.
        if check_state.forced_rollback_tick.is_some() {
            return;
        }
        match prediction_manager.rollback_config.mode {
            RollbackMode::Always => {
                scan.surveys = check_state.received_messages_this_frame && candidate.is_some();
                scan.counts_targets = scan.surveys;
            }
            RollbackMode::Check => {
                scan.surveys = candidate
                    .is_some_and(|candidate| check_state.has_confirmed_tick_advanced(candidate));
            }
            RollbackMode::Disabled => {}
        }
    }

    /// Asks for the rollback the frame decided on, or the one a component's mismatch at the scan
    /// tick asks for.
    fn finish_rollback_check(
        input_config: Res<'_, InputTimelineConfig>,
        prediction_manager: Res<'_, PredictionManager>,
        checkpoints: Res<'_, ReplicationCheckpointMap>,
        mut check_state: ResMut<'_, RollbackCheckState>,
        mut scan: ResMut<'_, RollbackScan>,
        mut commands: Commands<'_, '_>,
    ) {
        if let (Some(confirmed_tick), Some(entity)) = (scan.scan_tick, scan.mismatch) {
            let tick = scan.tick.expect("a scan runs once the timeline is synced");
            debug!(
                ?confirmed_tick,
                "Rollback because of mismatch at completed checkpoint"
            );
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "completed_checkpoint_mismatch",
                schedule = "PreUpdate",
                sample_point = "PreUpdate",
                entity = ?entity,
                local_tick = tick.0,
                latest_completed_tick = checkpoints.last_confirmed_tick().map(|tick| tick.0),
                completed_tick = confirmed_tick.0,
                rollback_tick = confirmed_tick.0,
                "rollback mismatch detected at completed checkpoint"
            );
            let max_rollback_ticks = prediction_manager.max_rollback_ticks(&input_config);
            scan.request(confirmed_tick, max_rollback_ticks, RollbackKind::Policy);
        }
        // if we have a rollback, despawn any PreSpawned/LocallyPredicted entities that were spawned
        // since the rollback tick (they will get respawned during the rollback)
        //
        // NOTE: if rollback happened at rollback_tick, then we will start running systems starting from
        // rollback_tick + 1. so if the entity was spawned at tick >= rollback_tick + 1, we despawn it,
        // and it can get respawned again
        if let Some(rollback_tick) = scan.requested {
            commands.insert_resource(Rollback(rollback_tick));
            check_state.rollback_kind = scan.kind;
        }
    }

    /// At the start of a rollback, despawns the pre-spawned and locally predicted entities that
    /// spawned after its tick, which the replay spawns again, and sets which locally predicted
    /// entities the rollback leaves alone.
    fn start_rollback(
        rollback: Res<'_, Rollback>,
        mut check_state: ResMut<'_, RollbackCheckState>,
        mut pre_spawned_receiver: ResMut<'_, PreSpawnedReceiver>,
        locally_predicted: Query<'_, '_, &LocallyPredicted>,
        mut protected: Local<'_, EntityHashSet>,
        mut commands: Commands<'_, '_>,
    ) {
        let rollback_tick = rollback.0;
        let ends_protection = check_state.rollback_kind == RollbackKind::Snapshot;
        debug!(
            ?rollback_tick,
            "Rollback! Despawning all PreSpawned/LocallyPredicted entities spawned after the rollback tick"
        );
        protected.clear();
        protected.extend(check_state.local_skip_despawn.iter().filter_map(
            |(protection_tick, entity)| (*protection_tick > rollback_tick).then_some(*entity),
        ));
        pre_spawned_receiver.despawn_pre_spawned_after_with(
            rollback_tick + 1,
            |entity| {
                protected.contains(&entity)
                    || (ends_protection
                        && locally_predicted
                            .get(entity)
                            .is_ok_and(|predicted| predicted.skip_despawn))
            },
            &mut commands,
        );

        // A locally predicted entity that spawned after the rollback tick is despawned: the replay
        // spawns it again. The older ones stay listed until they leave the rollback window.
        let first_after = check_state
            .local_despawn
            .partition_point(|(spawn_tick, _)| *spawn_tick <= rollback_tick);
        for (_, entity) in check_state.local_despawn.drain(first_after..) {
            commands.entity(entity).queue_silenced(DespawnReplicated);
        }

        // For skip_despawn, the tick is the first tick after which we should start enabling
        // rollbacks on the entity: an entity whose tick the rollback is past is enabled again and
        // leaves the list, and each remaining one gets `DisableRollback`.
        //
        // A snapshot is an explicit one-shot reconcile (e.g. a late-join snapshot) where the caller
        // has already deposited authoritative confirmed state at `rollback_tick` for every relevant
        // entity. Stamping `DisableRollback` on skip-despawn entities during such a reconcile would
        // exclude them from `RollbackPlugin::prepare_rollback` and leave their prediction history out of sync with
        // the rest of the world, so the snapshot ends the protection window of every one.
        if ends_protection {
            for (_, entity) in check_state.local_skip_despawn.drain(..) {
                commands.entity(entity).try_remove::<DisableRollback>();
            }
        } else {
            let split_idx = check_state
                .local_skip_despawn
                .partition_point(|(t, _)| *t <= rollback_tick);
            for (_, entity) in check_state.local_skip_despawn.drain(..split_idx) {
                commands.entity(entity).try_remove::<DisableRollback>();
            }
            for &(_, entity) in &check_state.local_skip_despawn {
                commands.entity(entity).try_insert(DisableRollback);
            }
        }
    }

    /// A locally predicted entity that spawned before the oldest tick a rollback may restore can no
    /// longer be despawned by one, and leaves the list.
    fn prune_local_despawn(
        manager: Res<'_, PredictionManager>,
        input_config: Res<'_, InputTimelineConfig>,
        timeline: Res<'_, LocalTimeline>,
        mut check_state: ResMut<'_, RollbackCheckState>,
    ) {
        let oldest = manager.oldest_rollback_tick(timeline.tick(), &input_config);
        let stale = check_state
            .local_despawn
            .partition_point(|(spawn_tick, _)| *spawn_tick < oldest);
        if stale > 0 {
            check_state.local_despawn.drain(..stale);
        }
    }
}

#[cfg(test)]
mod tests;
