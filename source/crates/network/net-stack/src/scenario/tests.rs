use bevy_ecs::prelude::{Entity, On, ResMut, Resource};
use bevy_ecs::relationship::RelationshipTarget;
use bevy_replicon::shared::backend::connected_client::NetworkIdMap;
use campfire_net_connection::Connected;
use campfire_net_connection::Disconnected;
use campfire_net_connection::DisconnectedReason;
use campfire_net_connection::NetworkingState;
use campfire_net_core::LinkDirection;
use campfire_net_emulator::{Phase, RateLimit};
use campfire_net_link::{Link, Server, Unlink};
use campfire_net_payload::{
    AppCode, OpenedPayload, PacketNumber, PayloadKind, ReceivePayload, SendPayload, UnlinkReason,
};
use core::time::Duration;

use campfire_store::internals::Parallel;

use crate::fixtures::full_stack::FullStack;
use crate::scenario::Scenario;
use crate::scenario::faults::{Faults, Latency, Loss, Outage};
use campfire_log::internals::LogCheck;
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_sync::internals::input_timeline_is_synced;

/// Runs the swarm scenario of each seed and fails with every broken seed, its faults, what it
/// broke, and the command that replays it.
///
/// The seeds run in parallel, so the default run fits the test budget: a seed is
/// single-threaded and its log check is the thread's.
fn run_swarm(seeds: impl IntoIterator<Item = u64>) {
    let seeds: Vec<u64> = seeds.into_iter().collect();
    let broken: Vec<String> = Parallel::map(seeds.len(), |index| broken_seed(seeds[index]))
        .into_iter()
        .flatten()
        .collect();
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

/// What the swarm scenario of `seed` broke, with its faults and the command that replays it, or
/// `None` when it kept every invariant.
fn broken_seed(seed: u64) -> Option<String> {
    let scenario = Scenario::swarm(seed);
    let outcome = scenario.run();
    (!outcome.failures.is_empty()).then(|| {
        format!(
            "seed {seed}, {} {:?} clients, {:?}:\n  {}\n  replay: \
             CAMPFIRE_SWARM_SEED={seed} cargo drive test swarm",
            scenario.clients,
            scenario.kind,
            scenario.faults,
            counted(&outcome.failures).join("\n  ")
        )
    })
}

/// Each distinct failure once, with how many times it happened when more than once.
fn counted(failures: &[String]) -> Vec<String> {
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for failure in failures {
        *counts.entry(failure).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(failure, count)| match count {
            1 => failure.to_owned(),
            _ => format!("{failure} ({count} times)"),
        })
        .collect()
}

/// `CAMPFIRE_SWARM_SEED` replays one seed; `CAMPFIRE_SWARM_SEEDS` runs that many seeds after
/// `first`; otherwise `count` seeds run.
fn swarm_seeds(first: u64, count: u64) -> Vec<u64> {
    if let Ok(seed) = std::env::var("CAMPFIRE_SWARM_SEED") {
        let seed: u64 = seed.parse().expect("CAMPFIRE_SWARM_SEED is a u64");
        return if first == 0 { vec![seed] } else { vec![] };
    }
    let count = std::env::var("CAMPFIRE_SWARM_SEEDS").map_or(count, |seeds| {
        seeds.parse().expect("CAMPFIRE_SWARM_SEEDS is a u64")
    });
    (first..first + count).collect()
}

#[test]
fn each_seed_of_the_swarm_keeps_every_invariant() {
    run_swarm(swarm_seeds(0, 12));
}

/// Every fault at once, so the canary and the error check see them all fire.
fn every_fault() -> Faults {
    Faults {
        latency: Some(Latency {
            latency: Duration::from_millis(80),
            jitter: Duration::from_millis(15),
        }),
        loss: Some(Loss {
            mean: 0.05,
            burst: 3.0,
        }),
        duplicate: Some(0.02),
        reorder: Some(0.05),
        rate: Some(RateLimit::new(100_000, 100_000)),
        outage: Some(Outage {
            start: Duration::from_millis(500),
            length: Duration::from_millis(400),
            direction: None,
        }),
    }
}

/// Two runs of one seed under every fault, in a plain packet session and in a sealed one, keep
/// every invariant and repeat each other's digest: the sealed bytes differ by TLS's key shares,
/// which TLS draws from the OS, and nothing of what the digest reads.
#[test]
fn one_seed_repeats_exactly_under_every_fault() {
    let scenarios = [ClientKind::Plain, ClientKind::Udp].map(|kind| Scenario {
        faults: every_fault(),
        kind,
        ..Scenario::swarm(7)
    });
    // The runs share no state, so they run in parallel, and the digests compare runs on
    // different threads too.
    let mut runs =
        Parallel::map(2 * scenarios.len(), |index| scenarios[index / 2].run()).into_iter();
    let runs = scenarios
        .each_ref()
        .map(|_| [(); 2].map(|()| runs.next().unwrap()));
    for (scenario, [first, second]) in scenarios.iter().zip(runs) {
        let kind = scenario.kind;
        assert!(first.failures.is_empty(), "{kind:?}: {:#?}", first.failures);
        assert!(first.dropped > 0, "{kind:?}: the faults dropped packets");
        assert_eq!(
            first.digest, second.digest,
            "{kind:?}: two runs of one seed differ"
        );
    }
}

/// The outage logs no warning or error.
#[test]
fn a_full_outage_times_out_and_the_client_connects_again_over_a_new_link() {
    drop(LogCheck::scoped(full_outage_then_reconnect));
}

fn full_outage_then_reconnect() {
    let mut setup = StepperSetup::single();
    setup.packet_session.keep_alive = Duration::from_millis(250);
    setup.packet_session.idle_timeout = Duration::from_secs(1);
    let mut stepper = Stepper::new(FullStack, setup);
    let link = stepper.client_link(0);
    let old_client_of = stepper.client_of_entity(0);
    let start = stepper.now();
    for direction in [LinkDirection::ClientToServer, LinkDirection::ServerToClient] {
        stepper
            .network()
            .add_phase(link, direction, Phase::outage(start));
    }
    // Nothing arrives after `start`: the silence passes the packet session's 1 s timeout on the
    // first frame after 1 s.
    stepper.run_for(Duration::from_millis(1100));
    assert_eq!(
        stepper
            .client(0)
            .get::<Disconnected>()
            .map(|disconnected| &disconnected.reason),
        Some(&DisconnectedReason::LinkFailed(UnlinkReason::TimedOut))
    );

    // The server despawned its link to the client, so that neither its server's links nor
    // `peer_map` hold it: a reconnect is a new link.
    let world = stepper.server_app().world();
    assert!(world.get_entity(old_client_of).is_err());
    assert!(
        world
            .get::<Server>(stepper.server_entity())
            .unwrap()
            .collection()
            .is_empty()
    );
    assert!(
        world
            .resource::<NetworkingState>()
            .peer_map
            .values()
            .all(|&entity| entity != old_client_of)
    );
    assert!(world.resource::<NetworkIdMap>().is_empty());

    stepper.reconnect(0);
    stepper.run_for(Duration::from_secs(2));
    assert!(stepper.client(0).contains::<Connected>());
    assert!(stepper.client_of(0).contains::<Connected>());
    assert!(input_timeline_is_synced(stepper.client_app(0).world()));
}

/// The reasons each `Unlink` of an app gave, in order.
#[derive(Resource, Debug, Default)]
struct Unlinks(Vec<(Entity, UnlinkReason)>);

/// A peer that sends a packet no honest peer sends, here one of the fourth payload kind, is
/// unlinked as a protocol violation, without an `error!`, and the client sees the server close
/// its packet session and disconnects.
#[test]
fn a_protocol_violation_unlinks_the_peer() {
    drop(LogCheck::scoped(|| {
        let mut stepper = Stepper::new(FullStack, StepperSetup::single());
        stepper.server_app_mut().init_resource::<Unlinks>();
        stepper.server_app_mut().add_observer(
            |unlink: On<'_, '_, Unlink>, mut unlinks: ResMut<'_, Unlinks>| {
                unlinks.0.push((unlink.entity, unlink.reason));
            },
        );
        let client_of = stepper.client_of_entity(0);
        stepper
            .client_of_mut(0)
            .get_mut::<Link>()
            .unwrap()
            .packet_session
            .push_opened(OpenedPayload {
                number: PacketNumber(u64::MAX),
                kind: PayloadKind::new(3),
                payload: ReceivePayload::Frozen(SendPayload::from_static(&[0xff; 32])),
            });
        stepper.frame_step(1);

        let world = stepper.server_app().world();
        assert_eq!(
            world.resource::<Unlinks>().0,
            [(client_of, UnlinkReason::ProtocolViolation)]
        );
        assert!(world.get_entity(client_of).is_err());
        stepper.run_for(Duration::from_millis(200));
        assert_eq!(
            stepper
                .client(0)
                .get::<Disconnected>()
                .map(|disconnected| disconnected.reason),
            Some(DisconnectedReason::LinkFailed(UnlinkReason::ClosedByPeer(
                AppCode::ENDED
            )))
        );
    }));
}
