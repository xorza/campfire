use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bevy_ecs::prelude::{Add, Entity, On, Query, ResMut, Resource};
use bevy_ecs::schedule::{Schedules, SingleThreadedExecutor};
use campfire_net_connection::Connected;
use campfire_net_connection::Disconnected;
use campfire_net_connection::DisconnectedReason;
use campfire_net_connection::HostClient;
use campfire_net_connection::Started;
use campfire_net_connection::Stop;
use campfire_net_connection::{LocalId, RemoteId};
use campfire_net_endpoint::Endpoint;
use campfire_net_interpolation::InterpolationTimeline;
use campfire_net_interpolation::internals::interpolation_stack::interpolation_timeline_is_synced;
use campfire_net_link::{LocalAddr, PeerAddr, Unlinked};
use campfire_net_messages::MessageManager;
use campfire_net_messages::internals::protocol::StringMessage;
use campfire_net_messages::{EventSender, MessageReceiver, MessageSender};
use campfire_net_payload::UnlinkReason;
use campfire_net_replication::{ReplicationReceiver, ReplicationSender, SenderHello};
use campfire_net_stepper::AppId;
use campfire_net_stepper::StepperConfig;
use campfire_net_stepper::{Order, Stepper};
use campfire_net_sync::internals::sync_stack::input_timeline_is_synced;
use campfire_net_sync::{LinkEstimator, LocalTimelineSync, RemoteTimeline};
use campfire_net_transport::Transport;
use tracing::dispatcher;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::{Layer, prelude::*};

use super::*;
use crate::client_plugins::ClientPlugins;
use crate::internals::full_stack::FullStack;
use crate::server_plugins::ServerPlugins;
use bevy::app::TaskPoolPlugin;
use bevy::state::app::StatesPlugin;
use bevy::time::TimePlugin;
use campfire_net_interpolation::InterpolationPlugin;
use campfire_net_interpolation::InterpolationRegistry;
use core::time::Duration;

/// Check that the client/server setup is correct:
/// - the various components we expect are present
#[test]
fn a_client_server_has_every_layers_components() {
    let stepper = Stepper::new(FullStack, StepperConfig::single());

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
    assert!(stepper.client(0).contains::<MessageManager>());
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
    assert!(stepper.client(0).contains::<LocalAddr>());
    assert!(stepper.client(0).contains::<PeerAddr>());
    assert!(stepper.client(0).contains::<LocalId>());
    assert!(stepper.client(0).contains::<RemoteId>());

    assert!(stepper.server().contains::<Started>());
    assert!(stepper.server().contains::<Endpoint<EmulatedWires>>());

    assert!(stepper.client_of(0).contains::<Transport>());
    assert!(stepper.client_of(0).contains::<MessageManager>());
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
    assert!(stepper.client_of(0).contains::<PeerAddr>());
    assert!(stepper.client_of(0).contains::<LocalId>());
    assert!(stepper.client_of(0).contains::<RemoteId>());
}

/// Check that the client/server setup is correct:
/// - the various components we expect are present
#[test]
fn a_host_server_has_every_layers_components() {
    let mut stepper = Stepper::new(FullStack, StepperConfig::host_server());
    stepper.frame_step(1);

    // Check that the various components we expect are present
    assert!(stepper.host_client().contains::<HostClient>());
    // The global synchronization state follows LocalTimeline directly for a host client.
    assert!(input_timeline_is_synced(stepper.server_app().world()));
    assert!(!stepper.host_client().contains::<LocalTimelineSync>());
    assert!(stepper.host_client().contains::<RemoteTimeline>());
    // TODO: update Interpolation to be disabled for host-clients!
    assert!(
        stepper
            .server_app()
            .world()
            .contains_resource::<InterpolationTimeline>()
    );
    assert!(!stepper.host_client().contains::<InterpolationTimeline>());
    assert!(interpolation_timeline_is_synced(
        stepper.server_app().world()
    ));
    assert!(stepper.host_client().contains::<Transport>());
    assert!(stepper.host_client().contains::<MessageManager>());
    assert!(
        stepper
            .host_client()
            .contains::<MessageSender<StringMessage>>()
    );
    // Message receivers are created lazily when the first payload arrives.
    assert!(
        !stepper
            .host_client()
            .contains::<MessageReceiver<StringMessage>>()
    );
    assert!(stepper.host_client().contains::<EventSender<SenderHello>>());
    // no need to replicate between the host-client and the server
    assert!(!stepper.host_client().contains::<ReplicationSender>());
    assert!(!stepper.host_client().contains::<ReplicationReceiver>());
    assert!(stepper.host_client().contains::<Connected>());
    assert!(stepper.host_client().contains::<LocalId>());
    assert!(stepper.host_client().contains::<RemoteId>());

    assert!(stepper.server().contains::<Started>());
    assert!(stepper.server().contains::<Endpoint<EmulatedWires>>());
}

#[derive(Debug, Clone, Default)]
struct ErrorCapture {
    count: Arc<AtomicUsize>,
}

impl ErrorCapture {
    fn clear(&self) {
        self.count.store(0, Ordering::Relaxed);
    }

    fn count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
}

impl<S: Subscriber> Layer<S> for ErrorCapture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        if *metadata.level() != Level::ERROR || !metadata.target().starts_with("campfire_net") {
            return;
        }
        self.count.fetch_add(1, Ordering::Relaxed);
    }
}

fn with_error_capture(f: impl FnOnce(&ErrorCapture)) {
    let capture = ErrorCapture::default();
    let subscriber = tracing_subscriber::registry().with(capture.clone());
    let dispatch = tracing::Dispatch::new(subscriber);
    dispatcher::with_default(&dispatch, || f(&capture));
}

fn use_single_threaded_schedules(stepper: &mut Stepper<FullStack>) {
    for app in stepper.apps_mut() {
        for (_, schedule) in app.world_mut().resource_mut::<Schedules>().iter_mut() {
            schedule.set_executor(SingleThreadedExecutor::new());
        }
    }
}

fn assert_no_stack_errors(capture: &ErrorCapture) {
    assert_eq!(
        capture.count(),
        0,
        "disconnection emitted unexpected stack error logs"
    );
}

/// Regression test for https://github.com/cBournhonesque/lightyear/issues/613.
///
/// The old `ServerConnections::disconnect` path removed the client from netcode before removing
/// its higher-level sender, which emitted an error every send interval. The current
/// server-requested path disconnects clients while stopping the server; it must clean up the
/// per-client entity without emitting any stack errors in that frame or subsequent frames.
#[test]
fn server_requested_disconnection_does_not_emit_error_logs() {
    with_error_capture(|capture| {
        let mut stepper = Stepper::new(FullStack, StepperConfig::single());
        use_single_threaded_schedules(&mut stepper);
        let server = stepper.server_entity();
        let server_client = stepper.client_of_entity(0);

        capture.clear();
        stepper
            .server_app_mut()
            .world_mut()
            .trigger(Stop::from(server));
        stepper.step(20, stepper.frame_interval(), Order::ServerFirst);

        assert!(
            stepper
                .server_app()
                .world()
                .get_entity(server_client)
                .is_err(),
            "server-side client entity should be removed after server-requested disconnection"
        );
        assert_no_stack_errors(capture);
    });
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
/// An IO layer reports an abruptly closed session as `Unlinked`. The server disconnects the
/// client with the link's reason and despawns its link, so no stale sender emits errors on later
/// frames.
#[test]
fn abrupt_unlink_does_not_emit_error_logs() {
    with_error_capture(|capture| {
        let mut stepper = Stepper::new(FullStack, StepperConfig::single());
        use_single_threaded_schedules(&mut stepper);
        let server_client = stepper.client_of_entity(0);
        stepper.server_app_mut().init_resource::<Disconnects>();
        stepper.server_app_mut().add_observer(record_disconnect);

        capture.clear();
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
        assert_no_stack_errors(capture);
    });
}

fn app_with(groups: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
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
    assert!(!app.is_plugin_added::<InterpolationPlugin>());
    assert!(app.world().contains_resource::<InterpolationRegistry>());
}

/// A host server adds both groups, at one tick rate, and builds the shared layers once.
#[test]
fn a_host_server_builds_the_shared_layers_once() {
    let app = app_with(|app| {
        app.add_plugins((ClientPlugins::default(), ServerPlugins::default()));
    });
    assert_eq!(
        app.world().resource::<SharedPluginsBuilt>().tick_rate,
        TickRate::try_from(60).unwrap()
    );
}

#[test]
#[should_panic(expected = "run at one tick rate")]
fn two_tick_rates_in_one_app_panic() {
    let _ = app_with(|app| {
        app.add_plugins((
            ClientPlugins::default(),
            ServerPlugins {
                tick_rate: TickRate::try_from(100).unwrap(),
            },
        ));
    });
}
