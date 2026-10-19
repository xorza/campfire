use super::*;
use bytes::BytesMut;
use campfire_net_core::Instant;
use campfire_net_emulator::internals::per_mille;
use campfire_net_emulator::{DuplexModel, EmulatedNetwork, LinkModel, WireClock};
use campfire_net_packets::PacketSessionSetup;
use campfire_net_packets::internals::{CLIENT, PlainHost};
use campfire_net_payload::{OutgoingPayload, PayloadFate, PayloadKind, PayloadToken, UnlinkReason};
use core::num::NonZeroU32;
use core::time::Duration;
use std::thread;
use std::time::Instant as OsInstant;

use crate::server_secret::ServerSecret;

const ALPN: &[u8] = b"campfire-test";

/// The time between two frames of each app.
const FRAME: Duration = Duration::from_millis(10);

/// The longest a test waits for the network on loopback, inside a test's bound of 1 s.
const PATIENCE: Duration = Duration::from_millis(800);

fn identity() -> ServerIdentity {
    ServerIdentity::raw_key(ServerSecret::new(Secret::new([7; 32])))
}

fn pinned() -> ServerTrust {
    ServerTrust::pinned(identity().tls_key().unwrap())
}

fn roomy() -> AdmissionSetup {
    AdmissionSetup::new(NonZeroU32::MAX, Duration::from_secs(1))
}

fn setup() -> UdpSetup {
    UdpSetup::new(ALPN)
}

/// Times short enough that an idle timeout passes inside a test: 4 keep-alives to it.
fn short() -> UdpSetup {
    UdpSetup {
        packet_session: PacketSessionSetup {
            keep_alive: Duration::from_millis(50),
            idle_timeout: Duration::from_millis(200),
            bandwidth: None,
        },
        ..setup()
    }
}

fn send<Io: DatagramIo>(host: &mut PlainHost<Io>, carrier: u32, token: u64, data: &[u8]) {
    let mut payload = BytesMut::zeroed(OutgoingPayload::HEADROOM);
    payload.extend_from_slice(data);
    host.link_mut(carrier)
        .expect("the carrier holds a packet session")
        .queues
        .send(PayloadToken(token), PayloadKind::new(1), payload);
}

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
        let network = EmulatedNetwork::new(WireClock::Driven);
        let pair = network.pair(models.with_seed(seed));
        let mut server =
            PlainHost::driven(EmulatedServer::new(identity(), setup.clone(), seed).serve(roomy()));
        server.io().add(pair.server);
        server.start(Instant::ZERO);
        let mut client = PlainHost::driven(emulated_client(
            pair.client,
            trust,
            setup,
            seed.wrapping_add(1),
        ));
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
    let mut sim = Sim::new(&pinned(), &setup());
    sim.frames(5, true);
    assert!(sim.is_linked());
    let link = sim.server.linked().next().unwrap();
    let binding = sim.client.link(CLIENT).unwrap().binding;
    assert!(binding.is_some());
    assert_eq!(sim.server.link(link).unwrap().binding, binding);
    send(&mut sim.client, CLIENT, 1, b"ping");
    sim.frames(2, true);
    send(&mut sim.server, link, 2, b"pong");
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
            &pinned(),
            &setup(),
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
    let mut sim = Sim::new(&pinned(), &setup());
    sim.frames(5, true);
    sim.frames(490, false);
    assert_eq!(sim.client.link(CLIENT).unwrap().ended, None);
    sim.frames(20, false);
    assert_eq!(
        sim.client.link(CLIENT).unwrap().ended,
        Some(UnlinkReason::TimedOut)
    );
}

/// A server and a client on loopback sockets, each run in real time by its IO thread.
#[derive(Debug)]
struct Loopback {
    server: PlainHost<UdpIo>,
    client: PlainHost<UdpIo>,
}

impl Loopback {
    fn new(setup: &UdpSetup) -> Self {
        let origin = OsInstant::now();
        let socket = ServerSocket::bind(
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            identity(),
            setup.clone(),
        )
        .unwrap();
        let address = socket.local_address();
        let mut server = PlainHost::os(socket.serve(roomy()), origin);
        server.start(server.now());
        let client_socket = ClientSocket::bind(address, pinned(), setup.clone()).unwrap();
        let mut client = PlainHost::os(client_socket.endpoint(), origin);
        client.start(client.now());
        Self { server, client }
    }

    /// Runs a frame of each host until `done` holds, within `PATIENCE`.
    fn pump(&mut self, mut done: impl FnMut(&Self) -> bool) {
        let start = OsInstant::now();
        while !done(self) {
            assert!(
                start.elapsed() < PATIENCE,
                "the condition held within the patience"
            );
            self.frames();
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn frames(&mut self) {
        self.client.frame(self.client.now());
        self.server.frame(self.server.now());
    }

    fn is_linked(&self) -> bool {
        self.client.link(CLIENT).is_some_and(|link| link.linked)
            && self.server.linked().count() == 1
    }
}

/// A client links over loopback sockets, and its payload crosses and is delivered.
#[test]
fn a_client_links_over_loopback_and_a_payload_crosses() {
    let mut loopback = Loopback::new(&setup());
    loopback.pump(Loopback::is_linked);
    send(&mut loopback.client, CLIENT, 1, b"over loopback");
    loopback.pump(|loopback| {
        loopback
            .client
            .link(CLIENT)
            .unwrap()
            .fates
            .contains(&PayloadFate::Delivered(PayloadToken(1)))
    });
    let link = loopback.server.linked().next().unwrap();
    assert_eq!(
        loopback.server.link(link).unwrap().opened,
        [b"over loopback".to_vec()]
    );
}

/// Hosts that run no frame for twice the idle timeout keep their link: their IO threads send the
/// keep-alives and acknowledge them.
#[test]
fn a_stalled_app_keeps_its_link() {
    let mut loopback = Loopback::new(&short());
    loopback.pump(Loopback::is_linked);
    thread::sleep(Duration::from_millis(400));
    loopback.frames();
    let client = loopback.client.link(CLIENT).unwrap();
    assert_eq!(client.ended, None);
    assert!(client.linked);
}

/// A second socket on a port the first holds is an error that names the address.
#[test]
fn a_port_in_use_is_an_error_that_names_its_address() {
    let first = ServerSocket::bind(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        identity(),
        setup(),
    )
    .unwrap();
    let address = first.local_address();
    let error = ServerSocket::bind(address, identity(), setup()).unwrap_err();
    assert_eq!(error.address, address);
}
