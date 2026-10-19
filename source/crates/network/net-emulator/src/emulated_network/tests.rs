use super::*;
use crate::link_model::LinkModel;
use campfire_math::Num;

/// One emulated link, its wire time moved by hand.
struct Harness {
    network: EmulatedNetwork,
    pair: EmulatedPair,
}

impl Harness {
    fn new(models: DuplexModel) -> Self {
        let network = EmulatedNetwork::new(WireClock::Driven);
        let pair = network.pair(models);
        Self { network, pair }
    }

    fn frame_at(&self, now: Duration) {
        self.network.set_time(now);
    }

    fn send_from_client(&self, payload: Bytes) {
        self.pair.client.send(core::iter::once(payload));
    }

    fn server_received(&self) -> Vec<Bytes> {
        let mut received = Vec::new();
        self.network.receive_ends(
            core::slice::from_ref(&self.pair.server),
            usize::MAX,
            |_, payload| {
                received.push(payload);
            },
        );
        received
    }

    const fn emulated_link(&self) -> EmulatedLinkId {
        self.pair.client.link()
    }
}

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn a_perfect_link_delivers_at_once() {
    let harness = Harness::new(DuplexModel::default());
    harness.send_from_client(Bytes::from_static(b"a"));
    assert_eq!(harness.server_received(), [Bytes::from_static(b"a")]);
}

#[test]
fn latency_holds_a_packet_until_it_is_due() {
    let harness = Harness::new(DuplexModel::symmetric(
        LinkModel::perfect().with_latency(ms(50)),
    ));
    // Sent at wire time 10 ms, due at 60 ms.
    harness.frame_at(ms(10));
    harness.send_from_client(Bytes::from_static(b"a"));
    harness.frame_at(ms(59));
    assert!(harness.server_received().is_empty());
    assert_eq!(
        harness
            .network
            .until_due_ends(core::slice::from_ref(&harness.pair.server)),
        Some(ms(1))
    );
    harness.frame_at(ms(60));
    assert_eq!(harness.server_received(), [Bytes::from_static(b"a")]);
}

#[test]
fn an_outage_loses_packets_until_the_recovery() {
    let harness = Harness::new(DuplexModel::default());
    let link = harness.emulated_link();
    let up = LinkDirection::ClientToServer;
    harness.network.add_phase(link, up, Phase::outage(ms(100)));
    harness
        .network
        .add_phase(link, up, Phase::recovery(ms(200), LinkModel::perfect()));
    harness.frame_at(ms(50));
    harness.send_from_client(Bytes::from_static(b"before"));
    harness.frame_at(ms(150));
    harness.send_from_client(Bytes::from_static(b"during"));
    harness.frame_at(ms(250));
    harness.send_from_client(Bytes::from_static(b"after"));
    assert_eq!(
        harness.server_received(),
        [Bytes::from_static(b"before"), Bytes::from_static(b"after")]
    );
}

#[test]
fn a_stall_releases_what_it_held_at_its_end() {
    let harness = Harness::new(DuplexModel::default());
    let link = harness.emulated_link();
    let up = LinkDirection::ClientToServer;
    harness
        .network
        .add_phase(link, up, Phase::stall(ms(0), LinkModel::perfect()));
    harness
        .network
        .add_phase(link, up, Phase::recovery(ms(300), LinkModel::perfect()));
    harness.frame_at(ms(100));
    harness.send_from_client(Bytes::from_static(b"1"));
    harness.frame_at(ms(200));
    harness.send_from_client(Bytes::from_static(b"2"));
    assert!(harness.server_received().is_empty(), "the stall holds both");
    harness.frame_at(ms(300));
    assert_eq!(
        harness.server_received(),
        [Bytes::from_static(b"1"), Bytes::from_static(b"2")],
        "one burst, in send order"
    );
}

/// Each end's MTU is the link's, and none without one. A phase replaces the conditions and not the
/// MTU: under a phase, a packet within the 1300 bytes the packet session was told arrives, and one
/// past them is dropped.
#[test]
fn the_link_mtu_is_each_ends_in_every_phase() {
    let harness = Harness::new(DuplexModel::perfect().with_mtu(1300));
    assert_eq!(harness.pair.server.mtu(), Some(1300));
    assert_eq!(harness.pair.client.mtu(), Some(1300));
    assert_eq!(Harness::new(DuplexModel::default()).pair.client.mtu(), None);

    let up = LinkDirection::ClientToServer;
    harness.network.add_phase(
        harness.emulated_link(),
        up,
        Phase::recovery(ms(0), LinkModel::perfect()),
    );
    harness.frame_at(ms(10));
    harness.send_from_client(Bytes::from(vec![0_u8; 1300]));
    harness.send_from_client(Bytes::from(vec![0_u8; 1301]));
    assert_eq!(
        harness
            .server_received()
            .iter()
            .map(Bytes::len)
            .collect::<Vec<_>>(),
        [1300]
    );
}

/// The client end of each link has its own address, which the server end reports as its peer's,
/// and every client end's peer is the one server's address.
#[test]
fn each_link_names_its_client_end_by_its_own_address() {
    let network = EmulatedNetwork::new(WireClock::Driven);
    let addresses = [(); 2].map(|()| {
        let pair = network.pair(DuplexModel::default());
        let address = pair.client.client_address();
        assert_eq!(pair.server.remote_address(), address);
        assert_eq!(pair.client.remote_address(), EmulatedIo::SERVER_ADDR);
        address
    });
    assert_ne!(addresses[0], addresses[1]);
}

/// A receive hands at most its bound and leaves the rest due, in order: of 3 due packets, a
/// bound of 2 hands the first two, and the next receive the third.
#[test]
fn a_receive_hands_at_most_its_bound() {
    let harness = Harness::new(DuplexModel::perfect());
    for payload in [b"1", b"2", b"3"] {
        harness.send_from_client(Bytes::from_static(payload));
    }
    let mut received = Vec::new();
    assert_eq!(
        harness.network.receive_ends(
            core::slice::from_ref(&harness.pair.server),
            2,
            |_, payload| received.push(payload)
        ),
        2
    );
    assert_eq!(
        received,
        [Bytes::from_static(b"1"), Bytes::from_static(b"2")]
    );
    assert_eq!(harness.server_received(), [Bytes::from_static(b"3")]);
}

/// A read over two server ends hands each datagram with the end it reached, and its bound holds
/// across them: of 3 due on the first link and 2 on the second, a bound of 4 hands the first
/// link's 3 and one of the second's.
#[test]
fn a_read_over_many_ends_bounds_them_together() {
    let network = EmulatedNetwork::new(WireClock::Driven);
    let pairs = [(); 2].map(|()| network.pair(DuplexModel::perfect()));
    for (pair, count) in pairs.iter().zip([3_u8, 2]) {
        pair.client
            .send((0..count).map(|byte| Bytes::from(vec![byte])));
    }
    let servers = [&pairs[0].server, &pairs[1].server].map(|server| EmulatedIo {
        network: server.network.clone(),
        link: server.link,
        side: server.side,
    });
    let mut read = Vec::new();
    assert_eq!(
        network.receive_ends(&servers, 4, |end, payload| read.push((end.link, payload))),
        4
    );
    let [first, second] = [pairs[0].server.link, pairs[1].server.link];
    assert_eq!(
        read,
        [
            (first, Bytes::from_static(&[0])),
            (first, Bytes::from_static(&[1])),
            (first, Bytes::from_static(&[2])),
            (second, Bytes::from_static(&[0])),
        ]
    );
}

#[test]
fn a_duplicated_packet_arrives_twice() {
    let harness = Harness::new(DuplexModel::symmetric(
        LinkModel::perfect().with_duplicate(Num::ONE),
    ));
    harness.send_from_client(Bytes::from_static(b"a"));
    assert_eq!(
        harness.server_received(),
        [Bytes::from_static(b"a"), Bytes::from_static(b"a")]
    );
}

#[test]
fn the_edge_preset_sends_at_its_rate_after_its_latency() {
    let harness = Harness::new(DuplexModel::edge());
    // Up at 25,000 B/s, a 1000 B packet sends in 40 ms, then waits the 440 ms latency: sent at
    // 10 ms, it arrives at 490 ms.
    harness.frame_at(ms(10));
    harness.send_from_client(Bytes::from(vec![0_u8; 1000]));
    harness.frame_at(ms(489));
    assert!(harness.server_received().is_empty());
    harness.frame_at(ms(490));
    assert_eq!(harness.server_received().len(), 1);
}

#[test]
fn counts_follow_each_packets_fate() {
    const MTU: usize = 1_200;
    let harness = Harness::new(
        DuplexModel::symmetric(LinkModel::perfect().with_duplicate(Num::ONE)).with_mtu(MTU),
    );
    harness.send_from_client(Bytes::from_static(b"a"));
    harness.send_from_client(Bytes::from(vec![0_u8; MTU + 1]));
    harness.frame_at(ms(10));
    harness.server_received();
    // "a" arrives twice; the other is a byte over the MTU, and its bytes count as sent.
    assert_eq!(
        harness
            .network
            .counts(harness.emulated_link(), LinkDirection::ClientToServer),
        DirectionCounts {
            sent: 2,
            sent_bytes: 1 + u64::try_from(MTU + 1).unwrap(),
            dropped: 1,
            duplicated: 1,
            delivered: 2,
        }
    );
}
