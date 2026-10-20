use super::*;
use bytes::BytesMut;
use campfire_math::SplitMix64;
use campfire_net_payload::{OutgoingPayload, PayloadKind, PayloadToken};
use core::net::{IpAddr, Ipv4Addr};
use core::time::Duration;

use crate::establish::PathCheck;
use crate::header::{Header, PacketType};
use crate::plain_establishment::PlainEstablishment;
use crate::protection::Protection;

impl<C: Carrier> EndpointCore<C> {
    /// The address of packet session `key`'s peer.
    fn address(&mut self, key: PeerKey) -> Option<SocketAddr> {
        self.peer(key).map(|peer| peer.address)
    }
}

fn at(millis: u64) -> Instant {
    Instant::from_elapsed(Duration::from_millis(millis))
}

const ID: ConnectionId = ConnectionId::from_random([0, 0, 0, 0, 0, 0, 0, 7]);

fn address(port: u16) -> SocketAddr {
    SocketAddr::from(([10, 0, 0, 1], port))
}

/// A server's establishment that routes by a connection ID, and takes each unknown peer as the
/// packet session of `ID`.
#[derive(Debug)]
struct RoutedEstablishment;

impl Establish for RoutedEstablishment {
    fn receive(&mut self, _: Instant, _: Datagram, _: &mut Establishing) -> ReadCost {
        ReadCost::Light
    }

    fn poll(&mut self, _: Instant, _: &mut Establishing) {}

    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    fn ended(&mut self, _: Option<ConnectionId>) {}
}

impl ServerEstablish for RoutedEstablishment {
    fn unknown(&mut self, _: Instant, datagram: &Datagram, out: &mut Establishing) {
        out.established.push(Established {
            peer: datagram.from,
            protection: Protection::Plain,
            prefix: None,
            route: Some(ID),
            reset_token: None,
            binding: None,
        });
    }

    fn set_accepting(&mut self, _: bool) {}

    fn path_check(&mut self) -> Option<&mut dyn PathCheck> {
        Some(self)
    }
}

impl PathCheck for RoutedEstablishment {
    fn challenge(&mut self) -> [u8; 8] {
        [3; 8]
    }
}

/// The client's packet session, whose datagrams start with the server's ID 7, and one payload of
/// it.
fn client_payload(client: &mut PacketSession, now: Instant) -> BytesMut {
    let mut queues = PacketSessionQueues::default();
    let mut payload = BytesMut::zeroed(OutgoingPayload::HEADROOM);
    payload.extend_from_slice(b"data");
    queues.send(PayloadToken(0), PayloadKind::new(0), payload);
    let mut out = Vec::new();
    client.take_over(&mut queues, now, &mut out);
    out.pop().unwrap().datagram
}

/// A hand-over sizes the payload by the MTU the carry named, less the packet session's overhead: a
/// plain server packet session's datagrams carry no prefix and no tag, only a header of at most
/// `Header::MAX_LEN`, so an MTU of 600 holds a payload of `600 − Header::MAX_LEN`, and one of 900
/// one of `900 − Header::MAX_LEN`.
#[test]
fn a_hand_over_sizes_its_payload_by_the_carried_mtu() {
    for mtu in [600, 900] {
        let mut core = EndpointCore::<u32>::new(
            PacketSessionSetup::default(),
            EndpointSide::Server {
                establish: Box::new(RoutedEstablishment),
            },
            None,
        );
        let mut client = PacketSession::new(
            Protection::Plain,
            Some(ID),
            PacketSessionSetup::default(),
            at(0),
        );
        let payload = client_payload(&mut client, at(0));
        core.set_time(EndpointTime::Driven(at(0)));
        core.pass(
            &mut vec![Datagram {
                from: address(1),
                payload,
            }],
            &mut Vec::new(),
        );
        let Some(News::Linked { key, .. }) = core.next_news() else {
            panic!("the first packet opens a packet session");
        };
        core.carry(key, 1, mtu);
        let mut queues = PacketSessionQueues::default();
        core.hand_over(key, &mut queues);
        assert_eq!(queues.max_payload(), mtu - Header::MAX_LEN);
    }
}

/// A server that routes by address checks no path: a packet from a new address is a new peer's,
/// which it takes as its first, and the first peer stays where it was, with no challenge sent.
#[test]
fn a_server_routed_by_address_takes_a_new_address_as_a_new_peer_and_challenges_none() {
    let mut establish = PlainEstablishment::default();
    establish.set_accepting(true);
    let mut core = EndpointCore::<u32>::new(
        PacketSessionSetup::default(),
        EndpointSide::Server {
            establish: Box::new(establish),
        },
        None,
    );
    let mut client = PacketSession::new(
        Protection::Plain,
        None,
        PacketSessionSetup::default(),
        at(0),
    );
    let mut out = Vec::new();
    let mut linked = Vec::new();
    for (tick, from) in [(0, address(1)), (10, address(2))] {
        let payload = client_payload(&mut client, at(tick));
        core.set_time(EndpointTime::Driven(at(tick)));
        core.pass(&mut vec![Datagram { from, payload }], &mut out);
        while let Some(news) = core.next_news() {
            if let News::Linked { key, address, .. } = news {
                linked.push((key, address));
            }
        }
    }
    let addresses: Vec<SocketAddr> = linked.iter().map(|&(_, address)| address).collect();
    assert_eq!(addresses, [address(1), address(2)]);
    assert_eq!(core.address(linked[0].0), Some(address(1)));
    // Each peer gets the one answer to its first packet, the same bytes; a challenge would be a
    // third datagram, to address 2.
    let sent: Vec<SocketAddr> = out.iter().map(|outbound| outbound.to).collect();
    assert_eq!(sent, [address(1), address(2)]);
    assert_eq!(out[0].datagram, out[1].datagram);
}

/// A peer routed by its ID whose newer packet comes from a new address is checked there: the
/// challenge goes to the candidate, the answer from it moves the peer's address, and the
/// server's next packets go there.
#[test]
fn a_peer_that_moves_is_checked_at_its_new_address_and_then_followed() {
    let mut core = EndpointCore::<u32>::new(
        PacketSessionSetup::default(),
        EndpointSide::Server {
            establish: Box::new(RoutedEstablishment),
        },
        None,
    );
    let mut client = PacketSession::new(
        Protection::Plain,
        Some(ID),
        PacketSessionSetup::default(),
        at(0),
    );
    let mut out = Vec::new();
    let first = client_payload(&mut client, at(0));
    core.set_time(EndpointTime::Driven(at(0)));
    core.pass(
        &mut vec![Datagram {
            from: address(1),
            payload: first,
        }],
        &mut out,
    );
    let key = match core.next_news() {
        Some(News::Linked {
            key, address: from, ..
        }) => {
            assert_eq!(from, address(1));
            key
        }
        news => panic!("the first packet opens a packet session: {news:?}"),
    };

    let moved = client_payload(&mut client, at(10));
    core.set_time(EndpointTime::Driven(at(10)));
    core.pass(
        &mut vec![Datagram {
            from: address(2),
            payload: moved,
        }],
        &mut out,
    );
    let challenge = out.pop().unwrap();
    assert_eq!(
        challenge.to,
        address(2),
        "the challenge goes to the candidate"
    );
    assert_eq!(core.address(key), Some(address(1)));

    let mut answers = Vec::new();
    let challenge = BytesMut::from(&*challenge.datagram);
    client.receive(at(11), challenge, Path::Peer, &mut answers);
    let answer = answers.pop().unwrap().datagram;
    core.set_time(EndpointTime::Driven(at(12)));
    core.pass(
        &mut vec![Datagram {
            from: address(2),
            payload: answer,
        }],
        &mut out,
    );
    assert_eq!(
        core.address(key),
        Some(address(2)),
        "the answer moved the peer"
    );

    let mut queues = PacketSessionQueues::default();
    queues.send(
        PayloadToken(1),
        PayloadKind::new(0),
        BytesMut::zeroed(OutgoingPayload::HEADROOM),
    );
    out.clear();
    core.set_time(EndpointTime::Driven(at(13)));
    core.take_over(key, &mut queues, &mut out);
    assert_eq!(out.pop().unwrap().to, address(2));
}

/// A client's establishment that opens a plain packet session at its connect, whose server's resets
/// end in `TOKEN`.
#[derive(Debug)]
struct ResetEstablishment;

const TOKEN: [u8; stateless_reset::TOKEN_LEN] = [9; stateless_reset::TOKEN_LEN];

impl Establish for ResetEstablishment {
    fn receive(&mut self, _: Instant, _: Datagram, _: &mut Establishing) -> ReadCost {
        ReadCost::Light
    }

    fn poll(&mut self, _: Instant, _: &mut Establishing) {}

    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    fn ended(&mut self, _: Option<ConnectionId>) {}
}

impl ClientEstablish for ResetEstablishment {
    fn connect(&mut self, _: Instant, server: SocketAddr, out: &mut Establishing) {
        out.established.push(Established {
            peer: server,
            protection: Protection::Plain,
            prefix: None,
            route: None,
            reset_token: Some(TOKEN),
            binding: None,
        });
    }
}

/// A datagram from the client's server that ends in the reset token ends its packet session as
/// `Reset`, and its carrier learns it; one a byte shorter than a reset is read as a packet
/// session's.
#[test]
fn a_reset_of_its_token_ends_a_clients_packet_session() {
    let server = address(1);
    let carrier = 4_u32;
    let mut core = EndpointCore::<u32>::new(
        PacketSessionSetup::default(),
        EndpointSide::Client {
            server,
            establish: Box::new(ResetEstablishment),
        },
        None,
    );
    let mut out = Vec::new();
    core.set_time(EndpointTime::Driven(at(0)));
    core.start(carrier, &mut out);
    assert!(matches!(core.next_news(), Some(News::Linked { .. })));
    let reset = |len: usize| {
        let mut datagram = BytesMut::zeroed(len - TOKEN.len());
        datagram.extend_from_slice(&TOKEN);
        Datagram {
            from: server,
            payload: datagram,
        }
    };
    core.set_time(EndpointTime::Driven(at(1)));
    core.pass(&mut vec![reset(stateless_reset::MIN_LEN - 1)], &mut out);
    assert_eq!(core.next_news(), None);
    core.set_time(EndpointTime::Driven(at(2)));
    core.pass(&mut vec![reset(stateless_reset::MIN_LEN)], &mut out);
    assert_eq!(
        core.next_news(),
        Some(News::Ended {
            carrier,
            reason: UnlinkReason::Reset
        })
    );
}

/// An IO that failed ends each carried packet session as `SocketFailed`, and the pass ends.
#[test]
fn an_io_failure_ends_each_packet_session_and_the_pass() {
    let carrier = 4_u32;
    let mut core = EndpointCore::<u32>::new(
        PacketSessionSetup::default(),
        EndpointSide::Client {
            server: address(1),
            establish: Box::new(ResetEstablishment),
        },
        None,
    );
    core.set_time(EndpointTime::Driven(at(0)));
    core.start(carrier, &mut Vec::new());
    assert!(matches!(core.next_news(), Some(News::Linked { .. })));
    assert!(!core.is_ended());
    core.fail();
    assert!(core.is_ended());
    assert_eq!(
        core.next_news(),
        Some(News::Ended {
            carrier,
            reason: UnlinkReason::SocketFailed
        })
    );
    assert_eq!(core.next_news(), None);
}

/// A wire between the test's client and the core that loses, delays, reorders and copies each
/// datagram by a seed: 20 % lost, each other one due 10 to 40 ms after its send, and 10 % of
/// those sent twice.
#[derive(Debug)]
struct FaultyWire {
    random: SplitMix64,
    in_flight: Vec<(Instant, SocketAddr, BytesMut)>,
}

impl FaultyWire {
    fn send(&mut self, now: Instant, to: SocketAddr, datagram: &[u8]) {
        if self.random.below(5) == 0 {
            return;
        }
        let copies = if self.random.below(10) == 0 { 2 } else { 1 };
        for _ in 0..copies {
            let delay = Duration::from_millis(10 + self.random.below(31));
            self.in_flight
                .push((now + delay, to, BytesMut::from(datagram)));
        }
    }

    /// The datagrams due by `now`, in the order they fall due.
    fn due(&mut self, now: Instant) -> Vec<(SocketAddr, BytesMut)> {
        self.in_flight.sort_by_key(|(due, ..)| *due);
        let split = self.in_flight.partition_point(|(due, ..)| *due <= now);
        self.in_flight
            .drain(..split)
            .map(|(_, to, datagram)| (to, datagram))
            .collect()
    }
}

/// A peer that moves from address 1 to address 2 under loss, delay, reorder and copies, for 32
/// seeds: while its address is still 1, the core sends address 2 nothing but path challenges;
/// the peer's address becomes 2 within 5 s; and once it is 2, the core's payloads go there. The
/// core's payloads to address 1 get no ACK, so its probe timeout stays near 1 s and its three
/// challenges go a second apart (RFC 9000 §8.2); seed 17 loses all three, and its check starts
/// again at the peer's next packet and passes at 3.17 s.
#[test]
fn a_moved_peer_is_checked_under_loss_reorder_and_duplicates() {
    for seed in 0..32 {
        let mut core = EndpointCore::<u32>::new(
            PacketSessionSetup::default(),
            EndpointSide::Server {
                establish: Box::new(RoutedEstablishment),
            },
            None,
        );
        let mut client = PacketSession::new(
            Protection::Plain,
            Some(ID),
            PacketSessionSetup::default(),
            at(0),
        );
        let mut out = Vec::new();
        let first = client_payload(&mut client, at(0));
        core.set_time(EndpointTime::Driven(at(0)));
        core.pass(
            &mut vec![Datagram {
                from: address(1),
                payload: first,
            }],
            &mut out,
        );
        let Some(News::Linked { key, .. }) = core.next_news() else {
            panic!("seed {seed}: the first packet opens a packet session");
        };
        let mut wire = FaultyWire {
            random: SplitMix64::new(seed),
            in_flight: Vec::new(),
        };
        let mut moved_at = None;
        let mut payloads_after_the_move = 0;
        for frame in 1..=500_u64 {
            let now = at(frame * 10);
            let mut transmits = Vec::new();
            transmits.push(Transmit {
                path: Path::Peer,
                datagram: client_payload(&mut client, now),
            });
            client.poll(now, &mut transmits);
            for transmit in transmits {
                wire.send(now, SERVER, &transmit.datagram);
            }
            let mut to_server = Vec::new();
            let mut to_client = Vec::new();
            for (to, datagram) in wire.due(now) {
                if to == SERVER {
                    to_server.push(Datagram {
                        from: address(2),
                        payload: datagram,
                    });
                } else {
                    to_client.push(datagram);
                }
            }
            out.clear();
            core.set_time(EndpointTime::Driven(now));
            core.pass(&mut to_server, &mut out);
            let mut queues = PacketSessionQueues::default();
            queues.send(
                PayloadToken(frame),
                PayloadKind::new(0),
                BytesMut::zeroed(OutgoingPayload::HEADROOM),
            );
            core.set_time(EndpointTime::Driven(now));
            core.take_over(key, &mut queues, &mut out);
            let moved = core.address(key) == Some(address(2));
            for outbound in &out {
                if outbound.to != address(2) {
                    continue;
                }
                let packet_type = Header::read(&outbound.datagram).unwrap().packet_type;
                if moved {
                    payloads_after_the_move += usize::from(packet_type == PacketType::Payload);
                } else {
                    assert_eq!(
                        packet_type,
                        PacketType::PathChallenge,
                        "seed {seed}: only a challenge goes to a candidate"
                    );
                }
                wire.send(now, address(2), &outbound.datagram);
            }
            if moved {
                moved_at.get_or_insert(frame);
            }
            let mut answers = Vec::new();
            for datagram in to_client {
                client.receive(now, datagram, Path::Peer, &mut answers);
            }
            for answer in answers {
                wire.send(now, SERVER, &answer.datagram);
            }
        }
        assert!(moved_at.is_some(), "seed {seed}: the peer never moved");
        assert!(
            payloads_after_the_move > 0,
            "seed {seed}: payloads follow the peer"
        );
    }
}

/// Where the test's client sends: its server.
const SERVER: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1);

/// A keep-alive as long as the idle timeout lets a quiet packet session time out, so the endpoint
/// refuses it when it is made, not at its first packet session.
#[test]
#[should_panic(expected = "a keep-alive less often than the idle timeout")]
fn an_endpoint_refuses_a_keep_alive_as_long_as_its_idle_timeout() {
    let setup = PacketSessionSetup {
        keep_alive: Duration::from_secs(5),
        ..PacketSessionSetup::default()
    };
    let _ = EndpointCore::<u32>::new(
        setup,
        EndpointSide::Client {
            server: address(1),
            establish: Box::new(ResetEstablishment),
        },
        None,
    );
}

/// A pass polls a packet session only once its timer is due. A client's packet session pings at its
/// open at 0, so its first deadline is the keep-alive at 1 s, before the probe at 1,024 ms: a pass
/// at 999 ms sends nothing, one at 1 s sends the keep-alive, and the next deadline is the
/// keep-alive at 2 s, with one entry left in the timers.
#[test]
fn a_pass_polls_a_packet_session_once_its_timer_is_due() {
    let mut core = EndpointCore::<u32>::new(
        PacketSessionSetup::default(),
        EndpointSide::Client {
            server: address(1),
            establish: Box::new(ResetEstablishment),
        },
        None,
    );
    let mut out = Vec::new();
    core.set_time(EndpointTime::Driven(at(0)));
    core.start(4_u32, &mut out);
    assert_eq!(out.len(), 1, "the open's ping");
    assert_eq!(core.next_deadline(), Some(at(1_000)));
    out.clear();
    core.set_time(EndpointTime::Driven(at(999)));
    core.pass(&mut Vec::new(), &mut out);
    assert!(out.is_empty());
    core.set_time(EndpointTime::Driven(at(1_000)));
    core.pass(&mut Vec::new(), &mut out);
    assert_eq!(out.len(), 1, "the keep-alive");
    assert_eq!(core.next_deadline(), Some(at(2_000)));
    assert_eq!(core.timers.len(), 1);
}
