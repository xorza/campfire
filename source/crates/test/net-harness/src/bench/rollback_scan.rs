use bevy_app::FixedUpdate;
use bevy_ecs::query::With;
use bevy_ecs::system::Query;
use campfire_measure::{Kind, Scale};
use campfire_net_connection::NetworkTarget;
use campfire_net_prediction::internals::{CompPredictionOnly, PredictionStack};
use campfire_net_prediction::{PredictionSetup, RollbackMode};
use campfire_net_replication::{Predicted, PredictionTarget, Replicate};
use campfire_net_stepper::{ClientKind, StepOrder, Stepper, StepperSetup};
use criterion::Criterion;

/// The predicted units of the client's player, as a Zero Hour army's.
const UNITS: usize = 400;

/// Frames for the client's prediction to settle ahead of the server.
const SETTLE: usize = 64;

/// A predicting client's rollback check over `UNITS` predicted units that stand still and one that
/// moves, server and client alike, so a confirmed update reaches the client each tick and no
/// prediction misses, as the scan of each confirmed tick visits every predicted unit: one frame
/// of both ends, whose client checks each confirmed tick, `idle_400_check`, or never,
/// `idle_400_disabled`, the two's difference the cost of the check.
pub(crate) fn rollback_scan(c: &mut Criterion) {
    let mut group = Kind::Integration.group(c, "rollback_scan", Scale::Frame);
    for (name, mode) in [
        ("idle_400_check", RollbackMode::Check),
        ("idle_400_disabled", RollbackMode::Disabled),
    ] {
        let mut stepper = None;
        group.bench_function(name, |b| {
            let stepper = stepper.get_or_insert_with(|| army(mode));
            b.iter(|| stepper.frame_step(1));
        });
    }
    group.finish();
}

/// The mover's step, a unit of a value of 1 or more, which each end runs on its own.
#[expect(
    clippy::float_arithmetic,
    reason = "the prediction stack's test component holds an `f32`, which no match reads"
)]
fn step_mover(mut units: Query<'_, '_, &mut CompPredictionOnly>) {
    for mut unit in &mut units {
        if unit.0 >= 1.0 {
            unit.0 += 1.0;
        }
    }
}

/// The client's step of its predicted mover, as the server steps its own.
#[expect(
    clippy::float_arithmetic,
    reason = "the prediction stack's test component holds an `f32`, which no match reads"
)]
fn step_predicted_mover(mut units: Query<'_, '_, &mut CompPredictionOnly, With<Predicted>>) {
    for mut unit in &mut units {
        if unit.0 >= 1.0 {
            unit.0 += 1.0;
        }
    }
}

/// A server and one client whose rollback check is `mode`, the client predicting `UNITS` still
/// units and one mover, settled.
fn army(mode: RollbackMode) -> Stepper<PredictionStack> {
    let mut stepper = Stepper::new(
        PredictionStack,
        StepperSetup::with_clients(ClientKind::Plain, 1),
    );
    stepper
        .server_app_mut()
        .add_systems(FixedUpdate, step_mover);
    stepper
        .client_app_mut(0)
        .add_systems(FixedUpdate, step_predicted_mover);
    let client = stepper.client_app_mut(0).world_mut();
    client.resource_mut::<PredictionSetup>().rollback_setup.mode = mode;
    let server = stepper.server_app_mut().world_mut();
    for at in 0..=UNITS {
        let value = if at == UNITS { 1.0 } else { 0.0 };
        server.spawn((
            CompPredictionOnly(value),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ));
    }
    stepper.step(SETTLE, stepper.frame_interval(), StepOrder::ServerFirst);
    stepper
}
