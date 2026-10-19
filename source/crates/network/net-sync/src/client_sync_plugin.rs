//! Syncs a client's clocks to its server.
use crate::local_timeline_sync::LocalTimelineSync;
use crate::local_timeline_sync_plugin::LocalTimelineSyncPlugin;
use crate::remote_timeline;
use crate::remote_timeline::RemoteTimeline;
use crate::timeline_sync_plugin::TimelineSyncPlugin;
use bevy_app::prelude::*;
use bevy_ecs::prelude::IntoScheduleConfigs;
use campfire_net_connection::Client;
use campfire_net_timeline::TimelineSystems;

/// Keeps a client's estimate of its server's timeline, and syncs the local timeline to it.
#[derive(Debug)]
pub struct ClientSyncPlugin;

impl Plugin for ClientSyncPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TimelineSyncPlugin>() {
            app.add_plugins(TimelineSyncPlugin);
        }

        app.register_required_components::<Client, RemoteTimeline>();

        // Network synchronization disciplines the application's single LocalTimeline even when
        // it has several network links.
        app.add_plugins(LocalTimelineSyncPlugin);

        // Register these after the local-sync plugin initializes its default
        // configuration. `ClientSyncPlugin` can be built before `TimelinePlugin` installs TickRate,
        // while every runtime configuration update, connection, and sync event happens after it.
        app.add_observer(LocalTimelineSync::recompute_input_delay_on_config_update);

        app.add_observer(RemoteTimeline::handle_connect);
        app.add_observer(remote_timeline::update_remote_timeline);
        app.add_systems(
            PreUpdate,
            remote_timeline::advance_remote_timeline.in_set(TimelineSystems::Advance),
        );
        app.add_systems(Last, remote_timeline::reset_received_packet_remote_timeline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_net_core::TickRate;

    use bevy_time::{TimePlugin, TimeUpdateStrategy};
    use core::time::Duration;

    use campfire_net_connection::NetworkingState;
    use campfire_net_timeline::TimelinePlugin;

    use bevy_ecs::prelude::*;
    use campfire_net_core::NetTick;
    use campfire_net_core::TickInstant;
    use campfire_net_link::Linked;
    use campfire_net_timeline::rollback::Rollback;
    use campfire_net_transport::TransportSystems;

    #[derive(Resource, Default)]
    struct SeenAtReceive(Option<TickInstant>);

    /// The remote timeline advances a frame's real time before the frame's packets are
    /// received, and during a rollback too.
    #[test]
    fn the_remote_timeline_advances_by_real_time() {
        let mut app = App::new();
        app.world_mut()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                10,
            )));
        app.init_resource::<NetworkingState>();
        app.add_plugins((
            TimePlugin,
            TimelinePlugin {
                tick_rate: TickRate::try_from(100).unwrap(),
            },
            ClientSyncPlugin,
        ));
        app.update();

        let e = app
            .world_mut()
            .spawn((RemoteTimeline::default(), Linked))
            .id();
        assert_eq!(
            app.world().get::<RemoteTimeline>(e).unwrap().now,
            TickInstant::zero()
        );
        app.init_resource::<SeenAtReceive>();
        app.add_systems(
            PreUpdate,
            (|timelines: Query<'_, '_, &RemoteTimeline>, mut seen: ResMut<'_, SeenAtReceive>| {
                seen.0 = timelines.iter().next().map(|timeline| timeline.now);
            })
            .in_set(TransportSystems::Receive),
        );
        app.insert_resource(Rollback(NetTick(0)));
        app.update();
        assert_eq!(
            app.world().get::<RemoteTimeline>(e).unwrap().now,
            TickInstant::lit("1.0")
        );
        assert_eq!(
            app.world().resource::<SeenAtReceive>().0,
            Some(TickInstant::lit("1.0"))
        );
    }
}
