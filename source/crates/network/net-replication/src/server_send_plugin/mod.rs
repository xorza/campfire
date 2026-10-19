use crate::component_registry::ComponentRegistry;
use crate::interpolation_yield::InterpolationYield;
use crate::replication_systems::ReplicationSystems;
use crate::replication_target::InterpolatedSend;
use crate::replication_target::InterpolationTarget;
use crate::replication_target::PredictedSend;
use crate::replication_target::PredictionTarget;
use crate::replication_target::Replicate;
use crate::replication_target::ReplicationTarget;
use crate::replication_target::set_host_marker;
use crate::replication_timer::ReplicationTimer;
use crate::visibility_bits::VisibilityBits;
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_replicon::prelude::{AppVisibilityExt, Replicated};
use bevy_replicon::server::ServerSystems;
use bevy_replicon::server::server_tick::ServerTick;
use bevy_replicon::server::visibility::registry::FilterRegistry;
use bevy_replicon::shared::replication::registry::ReplicationRegistry;
use bevy_time::Time;
use campfire_net_connection::HostClient;
use campfire_net_connection::RemoteId;
use campfire_net_timeline::LocalTimeline;
use campfire_net_timeline::Rollback;
use tracing::trace;

#[derive(Debug)]
pub(crate) struct ServerSendPlugin;

impl Plugin for ServerSendPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ComponentRegistry>();
        app.init_resource::<ReplicationRegistry>();
        app.init_resource::<FilterRegistry>();

        app.add_systems(
            RunFixedMainLoop,
            Self::update_replication_tick
                .in_set(ServerSystems::IncrementTick)
                .in_set(RunFixedMainLoopSystems::AfterFixedMainLoop)
                .run_if(not(Rollback::is_active)),
        );
        app.add_observer(ReplicationTarget::<()>::on_remove);
        app.add_observer(ReplicationTarget::<PredictedSend>::on_remove);
        app.add_observer(ReplicationTarget::<InterpolatedSend>::on_remove);
        app.add_observer(ReplicationTarget::<PredictedSend>::sync_host_marker);
        app.add_observer(ReplicationTarget::<InterpolatedSend>::sync_host_marker);
        app.add_observer(ReplicationTarget::<PredictedSend>::sync_host_markers_on_host_added);
        app.add_observer(ReplicationTarget::<InterpolatedSend>::sync_host_markers_on_host_added);
        app.add_observer(ReplicationTarget::<PredictedSend>::sync_host_markers_on_host_removed);
        app.add_observer(ReplicationTarget::<InterpolatedSend>::sync_host_markers_on_host_removed);
        app.add_observer(Self::sync_host_interpolated_on_prediction);
        app.add_observer(Self::sync_host_interpolated_on_prediction_removed);
        app.add_observer(InterpolationYield::follow_prediction);
        app.add_observer(InterpolationYield::warn_on_interpolation);
        app.add_observer(InterpolationYield::leave_with_prediction);

        // make sure that any ordering relative to ReplicationSystems is also applied to
        // ServerSystems
        app.configure_sets(
            PostUpdate,
            ServerSystems::Send.in_set(ReplicationSystems::Send),
        );

        app.register_required_components::<Replicate, Replicated>();
        // Replication targets are `VisibilityFilter`s: Replicon evaluates them on
        // insertion and on link changes, replacing the previous manual bit writes.
        app.add_visibility_filter::<Replicate>();
        app.init_resource::<VisibilityBits>();
        app.register_required_components::<PredictionTarget, PredictedSend>();
        app.add_visibility_filter::<PredictionTarget>();
        app.register_required_components::<InterpolationTarget, InterpolatedSend>();
        app.add_visibility_filter::<InterpolationTarget>();
        app.add_visibility_filter::<InterpolationYield>();
    }
}

impl ServerSendPlugin {
    /// A prediction target, inserted or replaced, gives a host client the `Interpolated` marker
    /// exactly while the interpolation target names it and the prediction target does not.
    pub(crate) fn sync_host_interpolated_on_prediction(
        trigger: On<'_, '_, Insert<PredictionTarget>>,
        targets: Query<'_, '_, (&InterpolationTarget, &PredictionTarget)>,
        hosts: Query<'_, '_, (Entity, Option<&RemoteId>), With<HostClient>>,
        mut commands: Commands<'_, '_>,
    ) {
        let Ok((target, prediction)) = targets.get(trigger.entity) else {
            return;
        };
        let targeted = hosts
            .iter()
            .any(|(host, remote)| target.targets_host(host, remote, Some(prediction)));
        set_host_marker::<InterpolatedSend>(&mut commands.entity(trigger.entity), targeted);
    }

    /// A prediction target that goes leaves each host client the interpolation target names its
    /// `Interpolated` marker.
    pub(crate) fn sync_host_interpolated_on_prediction_removed(
        trigger: On<'_, '_, Remove<PredictionTarget>>,
        targets: Query<'_, '_, &InterpolationTarget>,
        hosts: Query<'_, '_, (Entity, Option<&RemoteId>), With<HostClient>>,
        mut commands: Commands<'_, '_>,
    ) {
        if trigger.trigger().new_archetype.is_none() {
            return;
        }
        let Ok(target) = targets.get(trigger.entity) else {
            return;
        };
        let targeted = hosts
            .iter()
            .any(|(host, remote)| target.targets_host(host, remote, None));
        set_host_marker::<InterpolatedSend>(&mut commands.entity(trigger.entity), targeted);
    }

    /// Replication is triggered in Replicon every time the `ServerTick` is incremented.
    ///
    /// Run this once per app frame after the fixed loop has drained. Replicating
    /// from every app frame can produce multiple Replicon checkpoints for the same
    /// fixed network tick, while running once per fixed step can produce multiple
    /// sends in a catch-up frame. Replicon's tick is only a replication checkpoint;
    /// it is mapped back to the stack's fixed tick by `ReplicationCheckpointMap`.
    pub(crate) fn update_replication_tick(
        time: Res<'_, Time>,
        timeline: Res<'_, LocalTimeline>,
        mut replication_timer: ResMut<'_, ReplicationTimer>,
        mut replication_tick: ResMut<'_, ServerTick>,
        mut last_observed_local_tick: Local<'_, Option<u32>>,
        mut pending_send: Local<'_, bool>,
    ) {
        let new_tick = timeline.tick();
        let new_tick_raw = new_tick.0;
        let fixed_ran_this_frame = last_observed_local_tick
            .map(|previous_tick| new_tick_raw > previous_tick)
            .unwrap_or(new_tick_raw > 0);
        *last_observed_local_tick = Some(new_tick_raw);

        if replication_timer.timer.tick(time.delta()).just_finished() {
            *pending_send = true;
        }

        if !*pending_send || !fixed_ran_this_frame {
            trace!(
                target: "campfire_net_debug::timeline",
                kind = "server_replication_tick_skipped",
                schedule = "RunFixedMainLoop",
                sample_point = "AfterFixedMainLoop",
                local_tick = new_tick_raw,
                server_tick = ?replication_tick.get(),
                pending_send = *pending_send,
                fixed_ran_this_frame,
                "replication server tick not advanced"
            );
            return;
        }

        let previous_server_tick = replication_tick.get();
        replication_tick.increment();
        *pending_send = false;
        trace!(
            target: "campfire_net_debug::timeline",
            kind = "server_replication_tick",
            schedule = "RunFixedMainLoop",
            sample_point = "AfterFixedMainLoop",
            local_tick = new_tick_raw,
            server_tick = ?replication_tick.get(),
            previous_server_tick = ?previous_server_tick,
            "replication server tick advanced"
        );
    }
}

#[cfg(test)]
mod tests;
