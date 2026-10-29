use core::time::Duration;

use campfire_net_core::Instant;
use campfire_net_core::TimeSource;
use campfire_net_emulator::internals::per_mille;
use campfire_net_emulator::{DuplexModel, EmulatedNetwork, LinkModel};
use campfire_net_packets::EmulatedWires;
use campfire_net_packets::internals::{CLIENT, PlainHost};
use campfire_net_payload::{PayloadFate, PayloadToken, UnlinkReason};

use crate::server_trust::ServerTrust;
use crate::test_link::TestLink;
use crate::udp_client::UdpClient;
use crate::udp_setup::UdpSetup;

/// The time between two frames of each app.
const FRAME: Duration = Duration::from_millis(10);

/// A client and a server on the emulated network, stepped by hand on its wire time.
#[derive(Debug)]
struct Sim {
    network: EmulatedNetwork,
    client: PlainHost<EmulatedWires>,
    server: PlainHost<EmulatedWires>,
    now: Duration,
}

impl Sim {
    fn new(trust: &ServerTrust, setup: &UdpSetup) -> Self {
        Self::with_models(trust, setup, DuplexModel::default(), 1)
    }

    /// A client and a server whose wire follows `models`, and whose random numbers follow `seed`.
    fn with_models(trust: &ServerTrust, setup: &UdpSetup, models: DuplexModel, seed: u64) -> Self {
        let network = EmulatedNetwork::new(TimeSource::Driven);
        let pair = network.pair(models.with_seed(seed));
        let mut server =
            PlainHost::driven(TestLink::server(setup.clone()).emulated(TestLink::roomy(), seed));
        server.io().add(pair.server);
        server.start(Instant::ZERO);
        let client = UdpClient::new(trust.clone(), setup.clone());
        let mut client = PlainHost::driven(client.emulated(pair.client, seed.wrapping_add(1)));
        client.start(Instant::ZERO);
        Self {
            network,
            client,
            server,
            now: Duration::ZERO,
        }
    }

    /// Runs `n` frames, the client's first and the server's unless `server` is false.
    fn frames(&mut self, n: usize, server: bool) {
        for _ in 0..n {
            self.network.set_time(self.now);
            let now = Instant::from_elapsed(self.now);
            self.client.frame(now);
            if server {
                self.server.frame(now);
            }
            self.now += FRAME;
        }
    }

    fn is_linked(&self) -> bool {
        self.client.link(CLIENT).is_some_and(|link| link.linked)
            && self.server.linked().count() == 1
    }
}

/// A pinned client links over the emulated network in a sealed packet session: each end holds one
/// binding, the same, and a payload crosses each way and is delivered.
#[test]
fn a_sealed_client_links_and_payloads_cross_both_ways() {
    let mut sim = Sim::new(&TestLink::pinned(), &TestLink::setup());
    sim.frames(5, true);
    assert!(sim.is_linked());
    let link = sim.server.linked().next().unwrap();
    let binding = sim.client.link(CLIENT).unwrap().binding;
    assert!(binding.is_some());
    assert_eq!(sim.server.link(link).unwrap().binding, binding);
    sim.client.queue(CLIENT, 1, b"ping");
    sim.frames(2, true);
    sim.server.queue(link, 2, b"pong");
    sim.frames(6, true);
    assert_eq!(sim.server.link(link).unwrap().opened, [b"ping".to_vec()]);
    let client = sim.client.link(CLIENT).unwrap();
    assert_eq!(client.opened, [b"pong".to_vec()]);
    assert_eq!(client.fates, [PayloadFate::Delivered(PayloadToken(1))]);
}

/// The handshake under 10 % loss, 10 % duplicates and 20 ms of jitter on 20 ms each way, which
/// reorders, for 16 seeds: each client links in a sealed packet session within the idle timeout,
/// and both ends hold one binding, the same.
#[test]
fn the_handshake_links_under_loss_reorder_and_duplicates() {
    let model = LinkModel::perfect()
        .with_latency(Duration::from_millis(20))
        .with_jitter(Duration::from_millis(20))
        .with_fixed_loss(per_mille(100))
        .with_duplicate(per_mille(100));
    for seed in 0..16 {
        let mut sim = Sim::with_models(
            &TestLink::pinned(),
            &TestLink::setup(),
            DuplexModel::symmetric(model.clone()),
            seed,
        );
        let mut frames = 0;
        while !sim.is_linked() {
            assert!(frames < 500, "seed {seed}: no link within the idle timeout");
            sim.frames(1, true);
            frames += 1;
        }
        let link = sim.server.linked().next().unwrap();
        let binding = sim.client.link(CLIENT).unwrap().binding;
        assert!(binding.is_some(), "seed {seed}");
        assert_eq!(
            sim.server.link(link).unwrap().binding,
            binding,
            "seed {seed}"
        );
    }
}

/// A client whose server falls silent ends with `TimedOut` once it heard nothing for the idle
/// timeout, 5 s: 500 frames, and not one before.
#[test]
fn a_silent_server_is_lost_by_the_idle_timeout() {
    let mut sim = Sim::new(&TestLink::pinned(), &TestLink::setup());
    sim.frames(5, true);
    sim.frames(490, false);
    assert_eq!(sim.client.link(CLIENT).unwrap().ended, None);
    sim.frames(20, false);
    assert_eq!(
        sim.client.link(CLIENT).unwrap().ended,
        Some(UnlinkReason::TimedOut)
    );
}
