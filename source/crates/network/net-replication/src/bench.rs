//! The crate's criterion cases, which `benches/net-replication.rs` runs.
//!
//! `integration/<send or receive>_<insert or update>/…`: the server's or the client's update for a
//! frame that replicates `ENTITIES` entities, each timed alone.

use crate::fixtures::steps::PROPAGATION_FRAMES;
use bevy_ecs::query::With;
use campfire_measure::Kind;
use campfire_measure::Scale;
use campfire_net_connection::NetworkTarget;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use core::time::Duration;
use criterion::{BenchmarkId, Criterion};
use std::time::Instant;

use crate::fixtures::protocol::CompSimple;
use crate::fixtures::replication_stack::ReplicationStack;
use crate::replication_target::Replicate;

const ENTITIES: usize = 1000;
const CLIENTS: [usize; 6] = [0, 1, 2, 4, 8, 16];

/// The app whose update a case times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Server,
    Client,
}

/// What the timed frame replicates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    /// The entities' spawn.
    Insert,
    /// A change to every entity's component, after the spawn reached the client.
    Update,
}

/// One timed update.
#[derive(Clone, Copy, Debug)]
struct Case {
    clients: usize,
    change: Change,
    side: Side,
}

/// Runs each case of the crate whose id criterion's filter takes.
pub fn run(criterion: &mut Criterion) {
    campfire_measure::without_logs(criterion, updates);
}

fn updates(criterion: &mut Criterion) {
    let mut group = Kind::Integration.group(criterion, "send_insert", Scale::Frame);
    for clients in CLIENTS {
        let case = Case {
            clients,
            change: Change::Insert,
            side: Side::Server,
        };
        group.bench_function(
            BenchmarkId::from_parameter(campfire_measure::count(clients, "client")),
            |bencher| bencher.iter_custom(|iters| case.time(iters)),
        );
    }
    group.finish();
    let cases = [
        ("send_update", Change::Update, Side::Server),
        ("receive_insert", Change::Insert, Side::Client),
        ("receive_update", Change::Update, Side::Client),
    ];
    for (name, change, side) in cases {
        let case = Case {
            clients: 1,
            change,
            side,
        };
        let mut group = Kind::Integration.group(criterion, name, Scale::Frame);
        group.bench_function(
            BenchmarkId::from_parameter(campfire_measure::count(1, "client")),
            |bencher| bencher.iter_custom(|iters| case.time(iters)),
        );
        group.finish();
    }
}

impl Case {
    /// The total time of `iters` timed updates, each on a fresh stepper.
    fn time(self, iters: u64) -> Duration {
        let mut elapsed = Duration::ZERO;
        for _ in 0..iters {
            let mut stepper = Stepper::new(
                ReplicationStack,
                StepperSetup::with_plain_clients(self.clients),
            );
            let spawned = match self.change {
                Change::Insert => 0.0,
                Change::Update => 1.0,
            };
            let entities = vec![
                (
                    CompSimple(spawned),
                    Replicate::to_clients(NetworkTarget::All)
                );
                ENTITIES
            ];
            stepper.server_world_mut().spawn_batch(entities);
            if self.change == Change::Update {
                stepper.frame_step(PROPAGATION_FRAMES);
                let world = stepper.server_world_mut();
                for mut component in world
                    .query_filtered::<&mut CompSimple, With<Replicate>>()
                    .iter_mut(world)
                {
                    component.0 = 0.0;
                }
            }
            stepper.advance_time(stepper.frame_interval());
            let start = Instant::now();
            stepper.server_app_mut().update();
            if self.side == Side::Server {
                elapsed += start.elapsed();
            }
            for client in 0..stepper.client_count() {
                let client_app = stepper.client_app_mut(client);
                let start = Instant::now();
                client_app.update();
                if self.side == Side::Client {
                    elapsed += start.elapsed();
                }
                let world = client_app.world_mut();
                let replicated = world
                    .query::<&CompSimple>()
                    .iter(world)
                    .filter(|component| component.0 == 0.0)
                    .count();
                assert_eq!(replicated, ENTITIES, "the frame replicates every entity");
            }
        }
        elapsed
    }
}
