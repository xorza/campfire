use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy_ecs::entity::Entity;
use bytes::Bytes;
use quinn_proto::{Endpoint, EndpointConfig, ServerConfig};
use quinn_udp::BATCH_SIZE;
use zeroize::Zeroizing;

use crate::endpoint_state::{EndpointState, News};
use crate::link_times::LinkTimes;
use crate::outbox::Outbox;
use crate::peer::PeerKey;
use crate::pinned_key_verifier::PinnedKeyVerifier;
use crate::quic_endpoint::QuicEndpoint;
use crate::recv_batch::RecvBatch;
use crate::reset_key::ResetKey;
use crate::server_key::ServerKey;
use crate::{CID_LEN, LINK_MIN_MTU};

/// The fixture clock's step from one read to the next: a frame at 60 Hz.
const STEP: Duration = Duration::from_millis(16);
/// The most datagrams Linux's GSO sends, and its GRO joins, as one.
const SEGMENTS: usize = 64;
/// Every datagram the fixture's client sends: the largest a link carries on QUIC's minimum
/// path.
static PACKET: [u8; LINK_MIN_MTU] = [0xa5; LINK_MIN_MTU];
const SERVER: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 4433);
const CLIENT: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), 50_000);

/// A server's endpoint and a client's, connected in memory with a clock of their own and
/// seeded random numbers, so each run sends the same packets at the same times: what an
/// allocation audit of the server's thread measures, its work under the mutex for one read
/// (design 15, D3). No frame takes the news or datagrams of either.
#[derive(Debug)]
pub struct ThreadFixture {
    server: FixtureEnd,
    client: FixtureEnd,
    /// The client's connection.
    key: PeerKey,
    now: Instant,
}

/// One end: its state, the read its next pass takes, and what its last pass sent.
#[derive(Debug)]
struct FixtureEnd {
    state: EndpointState,
    batch: RecvBatch,
    filled: usize,
    outbox: Outbox,
    address: SocketAddr,
}

impl ThreadFixture {
    /// The two ends, their handshake complete.
    pub fn connected() -> ThreadFixture {
        let secret = Zeroizing::new([7; 32]);
        let server_key = ServerKey::derive(&secret);
        let server = QuicEndpoint::server_config(server_key.server_config(), LinkTimes::CAMPFIRE);
        let mut server = FixtureEnd::new(
            QuicEndpoint::endpoint_config(ResetKey::derive(&secret), CID_LEN),
            Some(Arc::new(server)),
            [1; 32],
            SERVER,
        );
        server.state.start_accepting();
        let mut client = FixtureEnd::new(
            QuicEndpoint::endpoint_config(ResetKey::random(), CID_LEN),
            None,
            [2; 32],
            CLIENT,
        );
        let now = Instant::now();
        let config = QuicEndpoint::client_config(
            PinnedKeyVerifier::new(&server_key.public()).client_config(),
            LinkTimes::CAMPFIRE,
        );
        let key = client
            .state
            .connect(now, config, SERVER, Entity::PLACEHOLDER)
            .expect("a client connects to an address");
        let mut fixture = ThreadFixture {
            server,
            client,
            key,
            now,
        };
        let mut connected = [false; 2];
        for _ in 0..16 {
            fixture.exchange();
            for (end, connected) in [&mut fixture.client, &mut fixture.server]
                .into_iter()
                .zip(&mut connected)
            {
                while let Some(news) = end.state.next_news() {
                    *connected |= matches!(news, News::Connected { .. });
                }
            }
            if connected == [true; 2] {
                break;
            }
        }
        assert_eq!(connected, [true; 2], "the handshake completes");
        // The handshake's last packets and their acknowledgments, so each later read holds
        // only what the client sends.
        for _ in 0..16 {
            if fixture.exchange() == 0 {
                return fixture;
            }
        }
        panic!("the ends did not fall quiet after the handshake");
    }

    /// What each end sends the other, the client's first, over as many reads as it takes, and
    /// a step of the clock: how many slots they read.
    fn exchange(&mut self) -> usize {
        self.client.pass(self.now);
        let slots = self.client.deliver_to(&mut self.server, self.now);
        self.server.pass(self.now);
        let slots = slots + self.server.deliver_to(&mut self.client, self.now);
        self.now += STEP;
        slots
    }

    /// The client sends `datagrams` datagrams, which its pass writes as GSO does and the
    /// server's reads take as GRO joins them: how many slots they fill.
    pub fn send(&mut self, datagrams: usize) -> usize {
        let connection = &mut self
            .client
            .state
            .peer(self.key)
            .expect("the client's connection lives")
            .connection;
        for _ in 0..datagrams {
            connection
                .datagrams()
                .send(Bytes::from_static(&PACKET), true)
                .expect("a linked connection carries a datagram of LINK_MIN_MTU");
        }
        self.client.pass(self.now);
        self.client.deliver_to(&mut self.server, self.now)
    }

    /// The server's pass over its read: what the thread does under the mutex.
    pub fn pass(&mut self) {
        self.server.pass(self.now);
    }

    /// Takes the datagrams the server received, as a frame does, sends its answer to the
    /// client, and moves the clock a step: how many datagrams it took.
    pub fn answer(&mut self) -> usize {
        let taken = self.server.state.take_datagrams();
        self.server.deliver_to(&mut self.client, self.now);
        self.now += STEP;
        taken
    }
}

impl FixtureEnd {
    /// An end on `address`, a server's with `server`, its random numbers from `seed`.
    fn new(
        config: EndpointConfig,
        server: Option<Arc<ServerConfig>>,
        seed: [u8; 32],
        address: SocketAddr,
    ) -> FixtureEnd {
        let payload =
            usize::try_from(config.get_max_udp_payload_size()).expect("a UDP payload fits usize");
        let endpoint = Endpoint::new(Arc::new(config), server, true, Some(seed));
        FixtureEnd {
            state: EndpointState::new(endpoint, SEGMENTS),
            batch: RecvBatch::detached(payload * SEGMENTS),
            filled: 0,
            outbox: Outbox::default(),
            address,
        }
    }

    fn pass(&mut self, now: Instant) {
        self.state
            .pass(now, self.batch.datagrams(self.filled), &mut self.outbox);
        self.filled = 0;
    }

    /// What the last pass sent, each transmit a slot of `to`'s reads: a read takes
    /// `BATCH_SIZE` slots, 1 where `quinn-udp` reads one datagram at a time, as on macOS and
    /// Windows, so `to` passes over a full read at `now` before the next, as its thread does.
    /// How many slots it delivered.
    fn deliver_to(&mut self, to: &mut FixtureEnd, now: Instant) -> usize {
        let from = self.address;
        let mut delivered = 0;
        self.outbox.drain(|destination, bytes, stride| {
            assert_eq!(destination, to.address, "the fixture has two ends");
            if to.filled == BATCH_SIZE {
                to.pass(now);
            }
            to.batch.load(to.filled, from, bytes, stride);
            to.filled += 1;
            delivered += 1;
        });
        delivered
    }
}
