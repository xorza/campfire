use super::*;
use crate::connection_stack::ConnectionStack;

#[test]
fn run_for_on_equal_intervals_repeats_frame_step() {
    let mut lockstep = Stepper::new(ConnectionStack, StepperConfig::single());
    let mut scheduled = Stepper::new(ConnectionStack, StepperConfig::single());
    lockstep.frame_step(100);
    scheduled.run_for(lockstep.frame_interval() * 100);
    assert_eq!(scheduled.now(), lockstep.now());
    assert_eq!(scheduled.server_tick(), lockstep.server_tick());
    assert_eq!(scheduled.client_tick(0), lockstep.client_tick(0));
}

/// Both orders run every app once a step. A client stalled for 30 ms runs no frame at 10 and
/// 20 ms; its clock went on, so its frame at 30 ms runs the three ticks it missed.
#[test]
fn a_step_runs_each_app_once_unless_stalled() {
    let mut stepper = Stepper::new(ConnectionStack, StepperConfig::single());
    let (client, server) = (stepper.client_tick(0), stepper.server_tick());
    stepper.step(2, stepper.tick_rate().period(), Order::ServerFirst);
    stepper.step(1, stepper.tick_rate().period(), Order::ClientsFirst);
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

/// A host client lives in the server's app: it has no app of its own and connects with `init`.
#[test]
fn a_host_client_has_no_app_and_connects() {
    let stepper = Stepper::new(ConnectionStack, StepperConfig::host_server());
    assert_eq!(stepper.client_count(), 1);
    assert!(stepper.host_client().contains::<Connected>());
    assert!(stepper.client(0).contains::<Connected>());
}

/// Client 1 disconnects through its stack: its own entity is disconnected, the server despawns its
/// link to it, and client 0 stays connected at its index.
#[test]
fn a_client_disconnects_through_the_stack() {
    let mut stepper = Stepper::new(
        ConnectionStack,
        StepperConfig::with_clients(vec![ClientKind::Raw; 2]),
    );
    let link = stepper.client_of_entity(1);
    stepper.disconnect(1);
    assert!(stepper.client(1).contains::<Disconnected>());
    assert!(stepper.server_app().world().get_entity(link).is_err());
    assert!(stepper.client(0).contains::<Connected>());
    assert!(stepper.client_of(0).contains::<Connected>());
}

#[derive(Debug, Clone, Copy)]
struct NeverReady;

impl Stack for NeverReady {
    fn add_client(&self, app: &mut App, tick_rate: TickRate) {
        ConnectionStack.add_client(app, tick_rate);
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
    Stepper::new(NeverReady, StepperConfig::single());
}

/// A deferred stepper leaves its client unconnected until `init`.
#[test]
fn a_deferred_stepper_connects_on_init() {
    let mut config = StepperConfig::single();
    config.init = Init::Deferred;
    let mut stepper = Stepper::new(ConnectionStack, config);
    assert!(!stepper.client(0).contains::<Connected>());
    stepper.init();
    assert!(stepper.client(0).contains::<Connected>());
}
