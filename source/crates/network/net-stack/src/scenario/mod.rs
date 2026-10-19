//! Seeded network scenarios and the invariants every run must keep.
//!
//! A scenario connects and syncs its clients on a perfect link, then runs a workload under its
//! faults: numbered messages on every channel mode both ways, and replicated entities spawned,
//! updated and despawned. At the heal the network turns perfect and the workload stops; after the
//! drain, the run checks each channel's guarantee, the replicated state, the connection and the
//! sync, and that no `error!` was logged.

pub(crate) mod faults;
pub(crate) mod guarantee;
pub(crate) mod message_log;
pub(crate) mod outcome;
pub(crate) mod replication_workload;
pub(crate) mod wire_cost;

#[cfg(test)]
mod tests;

use bevy::prelude::*;
use campfire_net_connection::Connected;
use campfire_net_core::Direction;
use campfire_net_transport::Transport;
use core::hash::{Hash, Hasher};
use core::time::Duration;
use std::collections::BTreeMap;
use std::hash::DefaultHasher;

use crate::internals::full_stack::FullStack;
use crate::scenario::faults::Faults;
use crate::scenario::message_log::MessageLog;
use crate::scenario::outcome::Outcome;
use crate::scenario::replication_workload::ReplicationWorkload;
use campfire_log::internals::{LogCheck, Scoped};
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperConfig;
use campfire_net_sync::internals::sync_stack::input_timeline_is_synced;

/// One seeded run: its clients and their session, its faults, how long they hold, and how long
/// the healed network gets to deliver what is still owed.
#[derive(Debug, Clone)]
pub(crate) struct Scenario {
    pub(crate) seed: u64,
    pub(crate) clients: usize,
    /// A plain session or a sealed one of the UDP protocol, for every client.
    pub(crate) kind: ClientKind,
    pub(crate) faults: Faults,
    pub(crate) faulty: Duration,
    pub(crate) drain: Duration,
}

impl Scenario {
    /// The swarm's scenario of `seed`: one or two clients, plain or sealed, and faults the seed
    /// picks.
    pub(crate) fn swarm(seed: u64) -> Self {
        Self {
            seed,
            clients: 1 + usize::try_from(seed % 2).unwrap(),
            kind: if (seed / 2).is_multiple_of(2) {
                ClientKind::Raw
            } else {
                ClientKind::Udp
            },
            faults: Faults::from_seed(seed),
            faulty: Duration::from_secs(3),
            drain: Duration::from_secs(4),
        }
    }

    /// The run's outcome, each warning or error it logged a failure of it.
    pub(crate) fn run(&self) -> Outcome {
        let Scoped {
            value: mut outcome,
            check,
        } = LogCheck::scoped(|| self.run_unchecked());
        let logged = check.untaken().into_iter();
        outcome
            .failures
            .extend(logged.map(|line| format!("logged {line}")));
        outcome
    }

    fn run_unchecked(&self) -> Outcome {
        let mut config = StepperConfig::with_clients(vec![self.kind; self.clients]);
        config.seed = self.seed;
        let mut stepper = Stepper::new(FullStack, config);
        self.start_workload(&mut stepper);

        let start = stepper.now();
        let heal = start + self.faulty;
        for client in 0..self.clients {
            let link = stepper.client_link(client);
            for direction in [Direction::ClientToServer, Direction::ServerToClient] {
                for phase in self.faults.phases(direction, start, heal) {
                    stepper.network().add_phase(link, direction, phase);
                }
            }
        }
        stepper.run_for(self.faulty);
        self.stop_workload(&mut stepper);
        stepper.run_for(self.drain);
        self.check(&mut stepper)
    }

    fn start_workload(&self, stepper: &mut Stepper<FullStack>) {
        let log = || MessageLog {
            sending: true,
            ..default()
        };
        for client in 0..self.clients {
            let entity = stepper.client_entity(client);
            let app = stepper.client_app_mut(client);
            app.add_systems(Update, (MessageLog::receive, MessageLog::send).chain());
            app.world_mut().entity_mut(entity).insert(log());
            let client_of = stepper.client_of_entity(client);
            stepper
                .server_app_mut()
                .world_mut()
                .entity_mut(client_of)
                .insert(log());
        }
        stepper.server_app_mut().add_systems(
            Update,
            (
                MessageLog::receive,
                MessageLog::send,
                ReplicationWorkload::step,
            )
                .chain(),
        );
        stepper
            .server_app_mut()
            .insert_resource(ReplicationWorkload::running());
    }

    fn stop_workload(&self, stepper: &mut Stepper<FullStack>) {
        for client in 0..self.clients {
            let entity = stepper.client_entity(client);
            let world = stepper.client_app_mut(client).world_mut();
            world.get_mut::<MessageLog>(entity).unwrap().sending = false;
            let client_of = stepper.client_of_entity(client);
            let server = stepper.server_app_mut().world_mut();
            server.get_mut::<MessageLog>(client_of).unwrap().sending = false;
        }
        stepper
            .server_app_mut()
            .world_mut()
            .resource_mut::<ReplicationWorkload>()
            .running = false;
    }

    fn check(&self, stepper: &mut Stepper<FullStack>) -> Outcome {
        let mut failures = Vec::new();
        self.check_backlogs(stepper, &mut failures);
        let mut digest = DefaultHasher::new();
        let mut dropped = 0;
        let server_state = ReplicationWorkload::server_state(stepper.server_app_mut().world_mut());
        hash_state(&server_state, &mut digest);
        for client in 0..self.clients {
            let label = format!("client {client}");
            if !stepper.client(client).contains::<Connected>() {
                failures.push(format!("{label} is not connected after the heal"));
            }
            if !input_timeline_is_synced(stepper.client_app(client).world()) {
                failures.push(format!("{label} is not synced after the heal"));
            }
            let client_world = stepper.client_app(client).world();
            let client_log = client_world
                .get::<MessageLog>(stepper.client_entity(client))
                .unwrap();
            let server_log = stepper
                .server_app()
                .world()
                .get::<MessageLog>(stepper.client_of_entity(client))
                .unwrap();
            client_log.check_sent_to(server_log, &mut failures, &format!("{label} to server"));
            server_log.check_sent_to(client_log, &mut failures, &format!("server to {label}"));
            client_log.received.hash(&mut digest);
            server_log.received.hash(&mut digest);
            let client_state =
                ReplicationWorkload::client_state(stepper.client_app_mut(client).world_mut());
            if client_state != server_state {
                failures.push(format!(
                    "{label} replicated {client_state:?}, the server holds {server_state:?}"
                ));
            }
            stepper.client_tick(client).0.hash(&mut digest);
            let link = stepper.client_link(client);
            for direction in [Direction::ClientToServer, Direction::ServerToClient] {
                let counts = stepper.network().counts(link, direction);
                counts.hash(&mut digest);
                dropped += counts.dropped;
            }
        }
        stepper.server_tick().0.hash(&mut digest);
        Outcome {
            failures,
            digest: digest.finish(),
            dropped,
        }
    }

    /// On the healed network, every reliable message pending at the drain's end is acknowledged
    /// within one more frame, no fragmented message is partial, and no more packets wait for
    /// their fate than that frame sent: a backlog that outlives the frame leaks.
    fn check_backlogs(&self, stepper: &mut Stepper<FullStack>, failures: &mut Vec<String>) {
        let ends = |stepper: &Stepper<FullStack>, client: usize| {
            [
                (
                    "client",
                    Direction::ClientToServer,
                    stepper
                        .client(client)
                        .get::<Transport>()
                        .unwrap()
                        .pending_reliable(),
                ),
                (
                    "server",
                    Direction::ServerToClient,
                    stepper
                        .client_of(client)
                        .get::<Transport>()
                        .unwrap()
                        .pending_reliable(),
                ),
            ]
        };
        let owed: Vec<_> = (0..self.clients)
            .map(|client| ends(stepper, client))
            .collect();
        // The packets each end's transport gave its session, as the session sends packets of its
        // own.
        let sent = |stepper: &Stepper<FullStack>, client: usize, direction| {
            let end = match direction {
                Direction::ClientToServer => stepper.client(client),
                Direction::ServerToClient => stepper.client_of(client),
            };
            end.get::<Transport>().unwrap().counts().sent_packets
        };
        let sent_before: Vec<_> = (0..self.clients)
            .map(|client| {
                [Direction::ClientToServer, Direction::ServerToClient]
                    .map(|direction| sent(stepper, client, direction))
            })
            .collect();
        stepper.frame_step(1);
        for client in 0..self.clients {
            for (index, (end, direction, still)) in ends(stepper, client).into_iter().enumerate() {
                let label = format!("client {client} {end}");
                let stale: Vec<_> = owed[client][index]
                    .2
                    .iter()
                    .filter(|message| still.contains(message))
                    .collect();
                if !stale.is_empty() {
                    failures.push(format!(
                        "{label} still owes {stale:?} a frame after the drain"
                    ));
                }
                let transport = match end {
                    "client" => stepper.client(client).get::<Transport>(),
                    _ => stepper.client_of(client).get::<Transport>(),
                };
                let backlog = transport.unwrap().backlog();
                let frame_sent = sent(stepper, client, direction) - sent_before[client][index];
                let frame_sent = usize::try_from(frame_sent).unwrap();
                if backlog.partial_fragments != 0 || backlog.tracked_packet_acks > frame_sent {
                    failures.push(format!(
                        "{label} waits for {backlog:?} after a frame that sent {frame_sent} packets"
                    ));
                }
            }
        }
    }
}

fn hash_state(state: &BTreeMap<String, f32>, digest: &mut DefaultHasher) {
    for (name, value) in state {
        name.hash(digest);
        value.to_bits().hash(digest);
    }
}
