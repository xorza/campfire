use crate::input_timeline_setup::InputTimelineSetup;
use crate::link_estimator::LinkEstimator;
use crate::local_timeline_sync::LocalTimelineSync;
use crate::remote_timeline::RemoteTimeline;
use crate::sync_sample::SyncSample;
use crate::sync_systems::SyncSystems;
use crate::timeline_sync::TimelineSync;
use bevy_app::{App, Last, Plugin, PostUpdate};
use bevy_ecs::prelude::*;
use bevy_time::{Fixed, Time, Virtual};
use campfire_net_connection::Client;
use campfire_net_connection::Connected;
use campfire_net_connection::Disconnected;
use campfire_net_connection::NetworkTopology;
use campfire_net_connection::NetworkingState;
use campfire_net_timeline::LocalTimeline;
use campfire_net_timeline::LocalTimelineShift;
use campfire_net_timeline::NetworkTimeline;
use campfire_net_timeline::TimelineRate;
use tracing::trace;

/// The speed `update_virtual_time` composed: the application's base, and the speed it wrote.
#[derive(Debug, Clone, Copy)]
struct ComposedSpeed {
    base: f32,
    written: f32,
}

/// Synchronizes the application-global `LocalTimeline` without maintaining a shadow clock.
#[derive(Debug, Default)]
pub(crate) struct LocalTimelineSyncPlugin;

impl LocalTimelineSyncPlugin {
    /// Reset synchronization controller state when a client connects.
    fn handle_connect(
        trigger: On<'_, '_, Add<Connected>>,
        clients: Query<'_, '_, (), With<Client>>,
        mut sync: ResMut<'_, LocalTimelineSync>,
    ) {
        if clients.get(trigger.entity).is_ok() {
            sync.reset();
        }
    }

    /// Clear client readiness when it disconnects.
    fn handle_disconnect(
        trigger: On<'_, '_, Add<Disconnected>>,
        clients: Query<'_, '_, (), With<Client>>,
        mut sync: ResMut<'_, LocalTimelineSync>,
    ) {
        if clients.get(trigger.entity).is_ok() {
            sync.set_synced(false);
        }
    }

    /// Paces `Time<Virtual>` by the controller's factor while a client is synchronized, else by 1.
    ///
    /// The application holds the base speed, as a game speed: the written speed is the base times
    /// the factor. A speed that differs from the one written last frame is the application's new
    /// base.
    fn update_virtual_time(
        networking: Res<'_, NetworkingState>,
        local_timeline: Res<'_, LocalTimeline>,
        sync: Res<'_, LocalTimelineSync>,
        mut virtual_time: ResMut<'_, Time<Virtual>>,
        mut composed: Local<'_, Option<ComposedSpeed>>,
    ) {
        let factor = if sync.is_synced() && networking.topology.is_client() {
            sync.relative_speed()
        } else {
            1.0
        };
        let current = virtual_time.relative_speed();
        let base = composed
            .filter(|composed| composed.written == current)
            .map_or(current, |composed| composed.base);
        let written = base * factor;
        virtual_time.set_relative_speed(written);
        *composed = Some(ComposedSpeed { base, written });
        trace!(
            target: "campfire_net_debug::sync",
            kind = "relative_speed",
            timeline = "LocalTimeline",
            tick = local_timeline.tick().0,
            base,
            relative_speed = factor,
            "local timeline synchronization speed applied to Virtual time"
        );
    }

    /// Select the topology's clock source and update [`LocalTimelineSync`].
    ///
    /// A client follows its sole authoritative server.
    #[expect(
        clippy::too_many_arguments,
        reason = "a Bevy system takes each resource it reads"
    )]
    fn sync_timelines(
        tick_rate: Res<'_, TimelineRate>,
        fixed_time: Res<'_, Time<Fixed>>,
        virtual_time: Res<'_, Time<Virtual>>,
        networking: Res<'_, NetworkingState>,
        setup: Res<'_, InputTimelineSetup>,
        local_timeline: Res<'_, LocalTimeline>,
        mut sync: ResMut<'_, LocalTimelineSync>,
        remotes: Query<'_, '_, (&RemoteTimeline, &LinkEstimator), (With<Client>, With<Connected>)>,
        mut commands: Commands<'_, '_>,
    ) {
        let local_now = local_timeline.now(&fixed_time);

        if networking.is_changed() {
            sync.reset();
            sync.set_synced(false);
        }

        let NetworkTopology::Client(client) = &networking.topology else {
            return;
        };
        // A paused clock takes no correction, as a speed does not move it and a step would move
        // a timeline whose error grows only by the pause.
        if virtual_time.is_paused() {
            return;
        }
        let Ok((remote, estimator)) = remotes.get(*client) else {
            return;
        };
        if !remote.has_received_packet() {
            return;
        }
        let Some(estimate) = estimator.estimate() else {
            return;
        };
        sync.set_link_input_delay(&setup, Some(estimate), tick_rate.0);
        let remote_estimate = remote.current_estimate();
        let before_speed = sync.relative_speed();
        let sample = SyncSample {
            remote,
            setup: &*setup,
            estimate,
            tick_rate: tick_rate.0,
        };
        if let Some(tick_span) = sync.sync(local_now, &sample) {
            trace!(
                target: "campfire_net_debug::sync",
                kind = "sync_adjustment",
                source = ?client,
                timeline = "LocalTimeline",
                local_tick = local_now.tick().0,
                remote_tick = remote.tick().0,
                remote_estimate = ?remote_estimate,
                tick_span,
                relative_speed = sync.relative_speed(),
                ?estimate,
                "local timeline sync emitted LocalTimelineShift"
            );
            commands.trigger(LocalTimelineShift { delta: tick_span });
        } else {
            trace!(
                target: "campfire_net_debug::sync",
                kind = "sync_sample",
                source = ?client,
                timeline = "LocalTimeline",
                local_tick = local_now.tick().0,
                remote_tick = remote.tick().0,
                remote_estimate = ?remote_estimate,
                relative_speed_before = before_speed,
                relative_speed = sync.relative_speed(),
                ?estimate,
                "local timeline sync sampled"
            );
        }
    }

    /// Apply a whole-tick synchronization correction to the authoritative [`LocalTimeline`].
    ///
    /// Input, prediction, and pre-spawn observers receive the same event and shift their indexed
    /// histories consistently with the world clock.
    fn handle_local_timeline_shift(
        trigger: On<'_, '_, LocalTimelineShift>,
        mut local_timeline: ResMut<'_, LocalTimeline>,
    ) {
        local_timeline.shift(trigger.delta);
        trace!(
            target: "campfire_net_debug::sync",
            kind = "local_timeline_shift_apply",
            timeline = "LocalTimeline",
            tick_span = trigger.delta,
            local_tick = local_timeline.tick().0,
            "applied LocalTimelineShift to LocalTimeline"
        );
    }
}

impl Plugin for LocalTimelineSyncPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalTimelineSync>();
        app.init_resource::<InputTimelineSetup>();

        app.add_observer(Self::handle_connect);
        app.add_observer(Self::handle_disconnect);
        app.add_observer(Self::handle_local_timeline_shift);
        app.add_systems(PostUpdate, Self::sync_timelines.in_set(SyncSystems::Sync));
        app.add_systems(Last, Self::update_virtual_time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn virtual_speed(app: &App) -> f32 {
        app.world().resource::<Time<Virtual>>().relative_speed()
    }

    /// The application's base speed survives the pacing: unsynchronized, 0.5 stays 0.5; a
    /// synchronized client at factor 1.25 runs at 0.5 * 1.25 = 0.625; and a base the
    /// application sets to 2 runs at 2 * 1.25 = 2.5.
    #[test]
    fn pacing_composes_with_the_application_base_speed() {
        let mut app = App::new();
        app.init_resource::<NetworkingState>();
        app.init_resource::<LocalTimeline>();
        app.init_resource::<LocalTimelineSync>();
        app.init_resource::<Time<Virtual>>();
        app.add_systems(Last, LocalTimelineSyncPlugin::update_virtual_time);
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_relative_speed(0.5);

        app.update();
        assert_eq!(virtual_speed(&app), 0.5);

        let client = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<NetworkingState>().topology =
            NetworkTopology::Client(client);
        {
            let mut sync = app.world_mut().resource_mut::<LocalTimelineSync>();
            sync.set_synced(true);
            sync.set_relative_speed(1.25);
        }
        app.update();
        assert_eq!(virtual_speed(&app), 0.625);
        app.update();
        assert_eq!(virtual_speed(&app), 0.625);

        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_relative_speed(2.0);
        app.update();
        assert_eq!(virtual_speed(&app), 2.5);
    }
}
