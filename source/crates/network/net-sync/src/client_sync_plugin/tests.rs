use core::time::Duration;

use bevy_ecs::prelude::*;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use campfire_math::TickRate;
use campfire_net_connection::NetworkingState;
use campfire_net_core::NetTick;
use campfire_net_core::TickInstant;
use campfire_net_link::Linked;
use campfire_net_timeline::Rollback;
use campfire_net_timeline::TimelinePlugins;
use campfire_net_transport::TransportSystems;

use super::*;
use crate::client_sync_plugins::ClientSyncPlugins;
use crate::sync_plugins::SyncPlugins;

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
        TimelinePlugins {
            tick_rate: TickRate::try_from(100).unwrap(),
        },
        SyncPlugins,
        ClientSyncPlugins,
    ));
    app.update();

    let e = app
        .world_mut()
        .spawn((RemoteTimeline::default(), Linked))
        .id();
    assert_eq!(
        app.world().get::<RemoteTimeline>(e).unwrap().now,
        TickInstant::ZERO
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
        TickInstant::from(NetTick(1))
    );
    assert_eq!(
        app.world().resource::<SeenAtReceive>().0,
        Some(TickInstant::from(NetTick(1)))
    );
}
