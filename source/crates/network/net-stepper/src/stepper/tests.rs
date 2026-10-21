#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` by value"
)]

use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Res, ResMut};
use tracing::warn;

use super::*;
use crate::connection_stack::ConnectionStack;

#[test]
fn a_run_for_of_equal_intervals_repeats_the_frame_step() {
    let mut lockstep = Stepper::new(ConnectionStack, StepperSetup::single());
    let mut scheduled = Stepper::new(ConnectionStack, StepperSetup::single());
    lockstep.frame_step(100);
    scheduled.run_for(lockstep.frame_interval() * 100, StepOrder::ClientsFirst);
    assert_eq!(scheduled.now(), lockstep.now());
    assert_eq!(scheduled.server_tick(), lockstep.server_tick());
    assert_eq!(scheduled.client_tick(0), lockstep.client_tick(0));
}

/// Both orders run every app once a step. A client stalled for 30 ms runs no frame at 10 and
/// 20 ms; its clock went on, so its frame at 30 ms runs the three ticks it missed.
#[test]
fn a_step_runs_each_app_once_unless_stalled() {
    let mut stepper = Stepper::new(ConnectionStack, StepperSetup::single());
    let (client, server) = (stepper.client_tick(0), stepper.server_tick());
    stepper.step(2, stepper.tick_rate().period(), StepOrder::ServerFirst);
    stepper.step(1, stepper.tick_rate().period(), StepOrder::ClientsFirst);
    assert_eq!(stepper.client_tick(0), client + 3);
    assert_eq!(stepper.server_tick(), server + 3);

    stepper.stall(AppId::Client(0), Duration::from_millis(30));
    stepper.frame_step(2);
    assert_eq!(
        stepper.client_tick(0),
        client + 3,
        "a stalled client runs no frame"
    );
    assert_eq!(stepper.server_tick(), server + 5);
    stepper.frame_step(1);
    assert_eq!(stepper.client_tick(0), client + 6);
}

/// Client 1 disconnects through its stack: its own entity is disconnected, the server despawns its
/// link to it, and client 0 stays connected at its index.
#[test]
fn a_client_disconnects_through_the_stack() {
    let mut stepper = Stepper::new(
        ConnectionStack,
        StepperSetup::with_clients(vec![ClientKind::Plain; 2]),
    );
    let link = stepper.client_of_entity(1);
    stepper.disconnect(1);
    assert!(stepper.client(1).contains::<Disconnected>());
    assert!(stepper.server_world().get_entity(link).is_err());
    assert!(stepper.client(0).contains::<Connected>());
    assert!(stepper.client_of(0).contains::<Connected>());
}

#[derive(Debug, Clone, Copy)]
struct NeverReady;

impl Stack for NeverReady {
    fn add_client(&self, app: &mut App, client: usize, tick_rate: TickRate) {
        ConnectionStack.add_client(app, client, tick_rate);
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        ConnectionStack.add_server(app, tick_rate);
    }

    fn add_protocol(&self, _: &mut App) {}

    fn client_link(&self, _: &mut EntityWorldMut<'_>) {}

    fn server_link(&self, _: &mut EntityWorldMut<'_>) {}

    fn is_ready(&self, _: &World) -> bool {
        false
    }
}

/// `init` that can not finish names the client and its state, instead of returning as if it
/// had.
#[test]
#[should_panic(expected = "client 0 is connected but not ready after 50 ticks")]
fn init_names_the_client_it_waits_for() {
    Stepper::new(NeverReady, StepperSetup::single());
}

/// A deferred stepper leaves its client unconnected until `init`.
#[test]
fn a_deferred_stepper_connects_on_init() {
    let mut setup = StepperSetup::single();
    setup.init = Init::Deferred;
    let mut stepper = Stepper::new(ConnectionStack, setup);
    assert!(!stepper.client(0).contains::<Connected>());
    stepper.init();
    assert!(stepper.client(0).contains::<Connected>());
}

/// `frame` runs one app's next frame alone: the client's moves the time one interval on and runs
/// one tick of it, the server running none; then the server's, due at that time, runs its tick and
/// moves no time.
#[test]
fn a_frame_runs_one_app_alone_at_its_next_frame() {
    let mut stepper = Stepper::new(ConnectionStack, StepperSetup::single());
    let (now, client, server) = (stepper.now(), stepper.client_tick(0), stepper.server_tick());
    let interval = stepper.frame_interval();
    let state = |stepper: &Stepper<ConnectionStack>| {
        (stepper.now(), stepper.client_tick(0), stepper.server_tick())
    };
    stepper.frame(AppId::Client(0));
    assert_eq!(state(&stepper), (now + interval, client + 1, server));
    stepper.frame(AppId::Server);
    assert_eq!(state(&stepper), (now + interval, client + 1, server + 1));
}

/// The apps a frame of `run_for` ran, in the order they ran.
#[derive(Resource, Debug)]
struct Ran {
    app: AppId,
    order: Arc<Mutex<Vec<AppId>>>,
}

impl Ran {
    fn record(ran: Res<'_, Self>) {
        ran.order.lock().unwrap().push(ran.app);
    }
}

/// The frames of `run_for` that fall together run in its order of ties: of a server and a client
/// of one interval, the client's first or the server's.
#[test]
fn run_for_runs_the_frames_that_fall_together_in_its_order() {
    let cases = [
        (StepOrder::ClientsFirst, [AppId::Client(0), AppId::Server]),
        (StepOrder::ServerFirst, [AppId::Server, AppId::Client(0)]),
    ];
    for (ties, ran) in cases {
        let mut stepper = Stepper::new(ConnectionStack, StepperSetup::single());
        let order = Arc::new(Mutex::new(Vec::new()));
        for app in [AppId::Server, AppId::Client(0)] {
            let app_of = match app {
                AppId::Server => stepper.server_app_mut(),
                AppId::Client(client) => stepper.client_app_mut(client),
            };
            let order = Arc::clone(&order);
            app_of.insert_resource(Ran { app, order });
            app_of.add_systems(Update, Ran::record);
        }
        stepper.run_for(stepper.frame_interval(), ties);
        assert_eq!(*order.lock().unwrap(), ran, "{ties:?}");
    }
}

/// A mark that a test's run put in a world.
#[derive(Resource, Debug)]
struct Marked;

/// Whether a system of the server's `FixedUpdate` saw the mark.
#[derive(Resource, Debug, Default)]
struct SawMark(bool);

impl SawMark {
    fn look(marked: Option<Res<'_, Marked>>, mut saw: ResMut<'_, Self>) {
        saw.0 |= marked.is_some();
    }
}

/// A stopped server runs no frame while its client runs on; a restarted one holds what `prepare`
/// put in it, and takes its client again, which connects through its new link. A run in the
/// server's fixed tick goes before its `FixedUpdate`, whose systems of that very tick see it.
#[test]
fn a_restarted_server_takes_its_client_again_and_a_fixed_run_comes_before_the_tick() {
    let mut stepper = Stepper::new(ConnectionStack, StepperSetup::single());
    stepper.disconnect(0);
    stepper.stop_server();
    let client = stepper.client_tick(0);
    stepper.frame_step(2);
    assert_eq!(stepper.client_tick(0), client + 2);
    assert!(!stepper.server_world().contains_resource::<LocalTimeline>());

    stepper.restart_server(|world| world.insert_resource(Marked));
    assert!(stepper.server_world().contains_resource::<Marked>());
    stepper.connect_client(0);
    for _ in 0..WAIT_TICKS {
        if stepper.client(0).contains::<Connected>() {
            break;
        }
        stepper.frame_step(1);
    }
    assert!(stepper.client_of(0).contains::<Connected>());

    let world = stepper.server_world_mut();
    world.remove_resource::<Marked>();
    world.init_resource::<SawMark>();
    stepper
        .server_app_mut()
        .add_systems(FixedUpdate, SawMark::look);
    stepper.server_fixed_update(|world| world.insert_resource(Marked));
    stepper.frame_step(1);
    assert!(stepper.server_world().resource::<SawMark>().0);
}

fn warn_each_frame() {
    warn!("a test app's warning");
}

/// A warning an app logs fails its test as the stepper drops, as no test took it.
#[test]
#[should_panic(expected = "the test logged events at Warn or Error that it did not take")]
fn an_untaken_warning_of_an_app_fails_as_the_stepper_drops() {
    let mut stepper = Stepper::new(ConnectionStack, StepperSetup::single());
    stepper
        .server_app_mut()
        .add_systems(Update, warn_each_frame);
    stepper.frame_step(1);
}
