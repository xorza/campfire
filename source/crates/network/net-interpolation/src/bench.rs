//! The crate's criterion cases, which `benches/net-interpolation.rs` runs.
//!
//! `integration/interpolation/<case>_moving`: the client's frames as `UNITS` units the server
//! replicates for it to interpolate change each frame, as a large battle's units move.

use std::time::Instant;

use bevy_ecs::query::With;
use campfire_measure::{Kind, RunCost, Scale};
use campfire_net_connection::NetworkTarget;
use campfire_net_replication::internals::PROPAGATION_FRAMES;
use campfire_net_replication::{Interpolated, InterpolationTarget, Replicate};
use campfire_net_stepper::{Stepper, StepperSetup};
use criterion::Criterion;

use crate::fixtures::interpolation_stack::InterpolationStack;
use crate::fixtures::protocol::CompFull;

/// The units of a run, as a large Zero Hour battle holds.
const UNITS: usize = 1000;
/// The frames of a run: 2 s at the stepper's 100 frames a second.
const FRAMES: u64 = 200;

/// Runs each case of the crate whose id criterion's filter takes.
pub fn run(criterion: &mut Criterion) {
    campfire_measure::without_logs(criterion, interpolation);
}

/// A run of `FRAMES` frames of a new stepper's client, its units replicated and interpolated, each
/// frame's server changing every unit, untimed: its worst frame, `worst_moving`, and its mean
/// frame, `mean_moving`.
fn interpolation(criterion: &mut Criterion) {
    let mut group = Kind::Integration.group(criterion, "interpolation", Scale::Frame);
    RunCost::cases(&mut group, "moving", FRAMES, moving);
    group.finish();
}

/// The cost of the client's frames of a run.
fn moving() -> RunCost {
    let mut stepper = Stepper::new(InterpolationStack, StepperSetup::single());
    let units = (
        CompFull(0.0),
        Replicate::to_clients(NetworkTarget::All),
        InterpolationTarget::to_clients(NetworkTarget::All),
    );
    stepper.server_world_mut().spawn_batch(vec![units; UNITS]);
    stepper.frame_step(PROPAGATION_FRAMES);
    let client = stepper.client_world_mut(0);
    let interpolated = client
        .query_filtered::<(), With<Interpolated>>()
        .iter(client)
        .count();
    assert_eq!(interpolated, UNITS, "the client interpolates every unit");
    RunCost::of(FRAMES, || {
        let server = stepper.server_world_mut();
        for mut unit in server.query::<&mut CompFull>().iter_mut(server) {
            unit.0 += 1.0;
        }
        stepper.advance_time(stepper.frame_interval());
        stepper.server_app_mut().update();
        let client = stepper.client_app_mut(0);
        let start = Instant::now();
        client.update();
        start.elapsed()
    })
}
