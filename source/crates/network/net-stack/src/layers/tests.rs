use bevy_ecs::prelude::{Add, Entity, On, Query, ResMut, Resource};
use campfire_log::internals::LogCheck;
use campfire_net_connection::Connected;
use campfire_net_connection::Disconnected;
use campfire_net_connection::DisconnectedReason;
use campfire_net_connection::Started;
use campfire_net_connection::Stop;
use campfire_net_connection::{LocalId, RemoteId};
use campfire_net_endpoint::Endpoint;
use campfire_net_interpolation::InterpolationTimeline;
use campfire_net_link::{LocalAddress, RemoteAddress, Unlinked};
use campfire_net_messages::LinkMessages;
use campfire_net_messages::internals::StringMessage;
use campfire_net_messages::{MessageReceiver, MessageSender};
use campfire_net_payload::UnlinkReason;
use campfire_net_replication::ReplicationSender;
use campfire_net_stepper::AppId;
use campfire_net_stepper::StepperSetup;
use campfire_net_stepper::{StepOrder, Stepper};
use campfire_net_sync::{LinkEstimator, LocalTimelineSync, RemoteTimeline};
use campfire_net_transport::Transport;

use super::*;
use crate::fixtures::full_stack::FullStack;
use crate::server_plugins::ServerPlugins;
use bevy::app::App;
use bevy::state::app::StatesPlugin;
use bevy::time::TimePlugin;
use campfire_app_threads::OneThreadPlugins;
use campfire_net_interpolation::InterpolationRegistry;
use core::time::Duration;

/// Check that the client/server setup is correct:
/// - the various components we expect are present
#[test]
fn a_client_server_has_every_layers_components() {
    let stepper = Stepper::new(FullStack, StepperSetup::single());

    // Check that the various components we expect are present
    assert!(stepper.client(0).contains::<LinkEstimator>());
    assert!(
        stepper
            .client_app(0)
            .world()
            .contains_resource::<LocalTimelineSync>()
    );
    assert!(!stepper.client(0).contains::<LocalTimelineSync>());
    assert!(stepper.client(0).contains::<RemoteTimeline>());
    assert!(
        stepper
            .client_app(0)
            .world()
            .contains_resource::<InterpolationTimeline>()
    );
    assert!(!stepper.client(0).contains::<InterpolationTimeline>());
    assert!(stepper.client(0).contains::<Transport>());
    assert!(stepper.client(0).contains::<LinkMessages>());
    assert!(stepper.client(0).contains::<MessageSender<StringMessage>>());
    // Message receivers are created lazily when the first payload arrives.
    assert!(
        !stepper
            .client(0)
            .contains::<MessageReceiver<StringMessage>>()
    );
    assert!(stepper.client(0).contains::<ReplicationSender>());
    assert!(stepper.client(0).contains::<Endpoint<EmulatedWires>>());
    assert!(stepper.client(0).contains::<Connected>());
    assert!(stepper.client(0).contains::<LocalAddress>());
    assert!(stepper.client(0).contains::<RemoteAddress>());
    assert!(stepper.client(0).contains::<LocalId>());
    assert!(stepper.client(0).contains::<RemoteId>());

    assert!(stepper.server().contains::<Started>());
    assert!(stepper.server().contains::<Endpoint<EmulatedWires>>());

    assert!(stepper.client_of(0).contains::<Transport>());
    assert!(stepper.client_of(0).contains::<LinkMessages>());
    assert!(
        stepper
            .client_of(0)
            .contains::<MessageSender<StringMessage>>()
    );
    assert!(
        !stepper
            .client_of(0)
            .contains::<MessageReceiver<StringMessage>>()
    );
    assert!(stepper.client_of(0).contains::<Connected>());
    assert!(stepper.client_of(0).contains::<RemoteAddress>());
    assert!(stepper.client_of(0).contains::<LocalId>());
    assert!(stepper.client_of(0).contains::<RemoteId>());
}

/// Regression test for https://github.com/cBournhonesque/lightyear/issues/613.
///
/// The old `ServerConnections::disconnect` path removed the client from netcode before removing
/// its higher-level sender, which emitted an error every send interval. The current
/// server-requested path disconnects clients while stopping the server; it must clean up the
/// per-client entity without emitting any stack errors in that frame or subsequent frames.
#[test]
fn a_disconnect_the_server_asks_for_logs_no_error() {
    let mut stepper = Stepper::new(FullStack, StepperSetup::single());
    let server = stepper.server_entity();
    let server_client = stepper.client_of_entity(0);
    drop(LogCheck::scoped(|| {
        stepper
            .server_app_mut()
            .world_mut()
            .trigger(Stop::from(server));
        stepper.step(20, stepper.frame_interval(), StepOrder::ServerFirst);

        assert!(
            stepper
                .server_app()
                .world()
                .get_entity(server_client)
                .is_err(),
            "server-side client entity should be removed after server-requested disconnection"
        );
    }));
}

#[derive(Debug, Resource, Default)]
struct Disconnects(Vec<(Entity, DisconnectedReason)>);

fn record_disconnect(
    trigger: On<'_, '_, Add<Disconnected>>,
    query: Query<'_, '_, &Disconnected>,
    mut disconnects: ResMut<'_, Disconnects>,
) {
    let reason = query.get(trigger.entity).unwrap().reason;
    disconnects.0.push((trigger.entity, reason));
}

/// Regression test for https://github.com/cBournhonesque/lightyear/issues/949.
///
/// An IO layer reports an abruptly closed packet session as `Unlinked`. The server disconnects the
/// client with the link's reason and despawns its link, so no stale sender emits errors on later
/// frames.
#[test]
fn an_abrupt_unlink_logs_no_error() {
    let mut stepper = Stepper::new(FullStack, StepperSetup::single());
    let server_client = stepper.client_of_entity(0);
    stepper.server_app_mut().init_resource::<Disconnects>();
    stepper.server_app_mut().add_observer(record_disconnect);
    drop(LogCheck::scoped(|| {
        // The client stops, as an abrupt close leaves it.
        stepper.stall(AppId::Client(0), Duration::from_secs(3600));
        stepper
            .server_app_mut()
            .world_mut()
            .entity_mut(server_client)
            .insert(Unlinked {
                reason: UnlinkReason::TimedOut,
            });

        stepper.frame_step(1);
        assert_eq!(
            stepper.server_app().world().resource::<Disconnects>().0,
            [(
                server_client,
                DisconnectedReason::LinkFailed(UnlinkReason::TimedOut)
            )]
        );
        assert!(
            stepper
                .server_app()
                .world()
                .get_entity(server_client)
                .is_err(),
            "the connection layer despawns the disconnected link"
        );

        // Keep running after the disconnection so a stale higher-level sender cannot hide
        // behind the disconnection frame.
        stepper.frame_step(20);
    }));
}

fn app_with(groups: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins((OneThreadPlugins, TimePlugin, StatesPlugin));
    groups(&mut app);
    app
}

/// A dedicated server holds the interpolation registry, which a shared protocol registers
/// into, and none of the interpolation systems, which only a client runs.
#[test]
fn a_dedicated_server_runs_no_interpolation() {
    let app = app_with(|app| {
        app.add_plugins(ServerPlugins::default());
    });
    assert!(!app.world().contains_resource::<InterpolationTimeline>());
    assert!(app.world().contains_resource::<InterpolationRegistry>());
}
