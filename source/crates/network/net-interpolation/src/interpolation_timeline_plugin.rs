use bevy_app::{App, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::prelude::*;
use bevy_time::{Real, Time, Virtual};
use campfire_net_connection::Client;
use campfire_net_connection::Connected;
use campfire_net_connection::Disconnected;
use campfire_net_connection::NetworkTopology;
use campfire_net_connection::NetworkingState;
use campfire_net_messages::RemoteEvent;
use campfire_net_replication::SenderHello;
use campfire_net_sync::LinkEstimator;
use campfire_net_sync::RemoteTimeline;
use campfire_net_sync::SyncSample;
use campfire_net_sync::SyncSystems;
use campfire_net_sync::TimelineSync;
use campfire_net_timeline::NetworkTimeline;
use campfire_net_timeline::TimelineRate;
use campfire_net_timeline::TimelineSystems;
use tracing::{debug, trace};

use crate::interpolation_setup::InterpolationSetup;
use crate::interpolation_timeline::InterpolationTimeline;

#[derive(Debug)]
pub(crate) struct InterpolationTimelinePlugin;

impl InterpolationTimelinePlugin {
    /// Reset the presentation timeline for a newly connected client.
    fn handle_connect(
        trigger: On<'_, '_, Add<Connected>>,
        clients: Query<'_, '_, (), With<Client>>,
        mut timeline: ResMut<'_, InterpolationTimeline>,
    ) {
        if clients.contains(trigger.entity) {
            timeline.reset_link();
        }
    }

    /// Reset presentation synchronization when a client disconnects.
    fn handle_disconnect(
        trigger: On<'_, '_, Add<Disconnected>>,
        clients: Query<'_, '_, (), With<Client>>,
        mut timeline: ResMut<'_, InterpolationTimeline>,
    ) {
        if clients.contains(trigger.entity) {
            timeline.reset_link();
        }
    }

    /// Cache the replication interval advertised by the client's one replication source.
    fn receive_sender_hello(
        trigger: On<'_, '_, RemoteEvent<SenderHello>>,
        tick_rate: Res<'_, TimelineRate>,
        networking: Res<'_, NetworkingState>,
        mut interpolation_timeline: ResMut<'_, InterpolationTimeline>,
    ) {
        let duration = trigger.event.send_interval.to_duration(tick_rate.0);
        if !networking.peer_map.contains_key(&trigger.from) {
            return;
        }
        debug!("Updating remote send interval to {:?}", duration);
        interpolation_timeline.state.remote_send_interval = duration;
    }

    /// Select the presentation source for the current topology and synchronize the resource.
    ///
    /// A client follows its sole server.
    fn sync_timeline(
        tick_rate: Res<'_, TimelineRate>,
        networking: Res<'_, NetworkingState>,
        setup: Res<'_, InterpolationSetup>,
        mut timeline: ResMut<'_, InterpolationTimeline>,
        remotes: Query<'_, '_, (&RemoteTimeline, &LinkEstimator), (With<Client>, With<Connected>)>,
    ) {
        if networking.is_changed() {
            // Source-set changes reset controller hysteresis without rewinding the presentation
            // cursor. The selected source below can immediately snap it to a new valid objective.
            timeline.reset();
        }

        match &networking.topology {
            NetworkTopology::Client(client) => {
                let Ok((remote, estimator)) = remotes.get(*client) else {
                    return;
                };
                if !remote.has_received_packet() {
                    return;
                }
                let Some(estimate) = estimator.estimate() else {
                    return;
                };
                let before = timeline.now();
                let tick_span = timeline.sync(
                    before,
                    &SyncSample {
                        remote,
                        setup: &setup,
                        estimate,
                        tick_rate: tick_rate.0,
                    },
                );
                trace!(
                    target: "campfire_net_debug::sync",
                    kind = "interpolation_sync",
                    source = ?client,
                    ?before,
                    after = ?timeline.now(),
                    remote_estimate = ?remote.current_estimate(),
                    ?tick_span,
                    relative_speed = timeline.relative_speed(),
                    "synchronized the application-global presentation timeline"
                );
            }
            NetworkTopology::Undefined
            | NetworkTopology::Server(_)
            | NetworkTopology::Invalid(_) => timeline.set_synced(false),
        }
    }

    /// Advances the timeline in `PreUpdate` by the frame's real time at the timeline's own speed,
    /// clamped and paused as [`Time<Virtual>`] is, but not at its speed, which the local
    /// timeline's synchronization sets; real time passes during a rollback too.
    pub(crate) fn advance_timeline(
        real: Res<'_, Time<Real>>,
        virtual_time: Res<'_, Time<Virtual>>,
        tick_rate: Res<'_, TimelineRate>,
        mut timeline: ResMut<'_, InterpolationTimeline>,
    ) {
        if virtual_time.effective_speed() == 0.0 {
            return;
        }
        let delta = real
            .delta()
            .min(virtual_time.max_delta())
            .mul_f32(timeline.relative_speed());
        trace!("Interpolation timeline advance by {delta:?}");
        timeline.apply_duration(delta, tick_rate.0);
    }
}

impl Plugin for InterpolationTimelinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InterpolationTimeline>();
        app.init_resource::<InterpolationSetup>();
        app.add_observer(Self::handle_connect);
        app.add_observer(Self::handle_disconnect);
        app.add_systems(PostUpdate, Self::sync_timeline.in_set(SyncSystems::Sync));
        app.add_systems(
            PreUpdate,
            Self::advance_timeline.in_set(TimelineSystems::Advance),
        );
        app.add_observer(Self::receive_sender_hello);
    }
}
