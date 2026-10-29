use core::time::Duration;

use campfire_net_connection::Connected;
use campfire_net_stepper::AppId;
use campfire_net_stepper::StepOrder;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_sync::internals::input_timeline_is_synced;

use super::*;

fn is_synced_and_connected(stepper: &Stepper<FullStack>) -> bool {
    input_timeline_is_synced(stepper.client_world(0)) && stepper.client(0).contains::<Connected>()
}

#[test]
fn a_client_on_a_faster_frame_rate_stays_synced() {
    let mut stepper = Stepper::new(FullStack, StepperSetup::single());
    // About 144 Hz on the client against the server's 100 Hz ticks.
    stepper.set_frame_interval(AppId::Client(0), Duration::from_millis(7));
    let start_tick = stepper.server_tick();
    stepper.run_for(Duration::from_secs(2), StepOrder::ClientsFirst);
    assert!(is_synced_and_connected(&stepper));
    // 2 s at the 10 ms tick is 200 server ticks.
    assert_eq!(stepper.server_tick() - start_tick, 200);
}

#[test]
fn a_client_whose_clock_drifts_stays_synced() {
    let mut stepper = Stepper::new(FullStack, StepperSetup::single());
    // 0.1 % fast: 4 ms over the 4 s run.
    stepper.set_clock_drift(AppId::Client(0), 1000);
    stepper.run_for(Duration::from_secs(4), StepOrder::ClientsFirst);
    assert!(is_synced_and_connected(&stepper));
}

#[test]
fn a_stalled_client_syncs_again() {
    let mut stepper = Stepper::new(FullStack, StepperSetup::single());
    let frames_before = stepper.client_tick(0);
    // A 2 s stall: Bevy advances the client's virtual time by at most 250 ms on the frame it
    // resumes, so its ticks fall behind the server's and the sync must bring them back.
    stepper.stall(AppId::Client(0), Duration::from_secs(2));
    stepper.run_for(Duration::from_millis(1990), StepOrder::ClientsFirst);
    assert_eq!(
        stepper.client_tick(0),
        frames_before,
        "a stalled client runs no frame"
    );
    stepper.run_for(Duration::from_secs(3), StepOrder::ClientsFirst);
    assert!(is_synced_and_connected(&stepper));
    // The synced client leads the server, which ran on through the stall.
    assert!(stepper.client_tick(0) >= stepper.server_tick());
}
