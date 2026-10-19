use super::*;
use bevy_app::{App, Update};
use bevy_ecs::prelude::{Resource, With};
use bevy_ecs::system::ResMut;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use bytes::BytesMut;
use campfire_net_core::Instant;
use campfire_net_emulator::{
    DuplexModel, EmulatedIo, EmulatedNetwork, LinkConditioning, LinkModel, WireClock,
};
use campfire_net_link::{LinkPlugin, Unlinked};
use campfire_net_packets::EmulatedWires;
use campfire_net_packets::{
    ChannelBinding, ClientEstablish, ConnectionId, Datagram, EndpointConfig, Establish,
    Established, Establishing, Protection, ServerEstablish,
};
use campfire_net_payload::{OutgoingPayload, PayloadFate, PayloadKind, PayloadToken};
use campfire_net_timeline::ClockPlugin;
use campfire_net_timeline::ClockSource;
use core::net::SocketAddr;
use core::time::Duration;

use crate::endpoint_plugin::EndpointPlugin;

/// The time between two frames of each app.
const FRAME: Duration = Duration::from_millis(10);

/// What a test's frame did with its links: each opened payload, which it accepts when `accept`
/// holds, and each fate.
#[derive(Resource, Debug, Default)]
struct Inbox {
    refuse: bool,
    opened: Vec<(Entity, Vec<u8>)>,
    fates: Vec<(Entity, PayloadFate)>,
    rtt_samples: Vec<(Entity, Duration)>,
}

impl Inbox {
    /// Takes, in the frame between its two steps, what each link's session handed it.
    fn take(mut links: Query<'_, '_, (Entity, &mut Link)>, mut inbox: ResMut<'_, Self>) {
        for (entity, mut link) in &mut links {
            let opened: Vec<_> = link.session.opened().collect();
            for payload in opened {
                if !inbox.refuse {
                    link.session.accept(payload.number);
                }
                inbox
                    .opened
                    .push((entity, payload.payload.freeze().to_vec()));
            }
            inbox
                .fates
                .extend(link.session.fates().map(|fate| (entity, fate)));
            let samples = link.session.rtt_samples();
            inbox
                .rtt_samples
                .extend(samples.iter().map(|sample| (entity, *sample)));
        }
    }

    /// The fate of each token, in token order.
    fn fates_by_token(&self) -> Vec<PayloadFate> {
        let mut fates: Vec<_> = self.fates.iter().map(|(_, fate)| *fate).collect();
        fates.sort_by_key(|fate| match fate {
            PayloadFate::Delivered(token) | PayloadFate::Lost(token) => *token,
        });
        fates
    }
}

/// A payload of `data` with its headroom.
fn payload(data: &[u8]) -> BytesMut {
    let mut payload = BytesMut::zeroed(OutgoingPayload::HEADROOM);
    payload.extend_from_slice(data);
    payload
}

/// A client's app and a server's, each with a plain endpoint over one simulated link, stepped by
/// hand on the network's wire time.
#[derive(Debug)]
struct Harness {
    network: EmulatedNetwork,
    client: App,
    server: App,
    client_entity: Entity,
    now: Duration,
}

impl Harness {
    fn new(models: DuplexModel) -> Self {
        Self::with_server(models, EndpointConfig::default())
    }

    fn with_server(models: DuplexModel, server_config: EndpointConfig) -> Self {
        Self::with_endpoints(models, EndpointParts::plain_server(server_config), |wire| {
            EndpointParts::plain_client(wire, EndpointConfig::default())
        })
    }

    /// A server's endpoint of `server_parts`, and a client's that `client_parts` makes of its
    /// wire.
    fn with_endpoints(
        models: DuplexModel,
        server_parts: EndpointParts<EmulatedWires>,
        client_parts: impl FnOnce(EmulatedIo) -> EndpointParts<EmulatedWires>,
    ) -> Self {
        let network = EmulatedNetwork::new(WireClock::Driven);
        let pair = network.pair(models);
        let app = || {
            let mut app = App::new();
            app.insert_resource(Clock::new(ClockSource::Frame));
            app.add_plugins((
                TimePlugin,
                ClockPlugin,
                LinkPlugin,
                EndpointPlugin::<EmulatedWires>::default(),
            ));
            app.insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
            app.init_resource::<Inbox>();
            app.add_systems(Update, Inbox::take);
            app
        };
        let mut client = app();
        let mut server = app();
        let server_endpoint = Endpoint::from(server_parts);
        server_endpoint.io().add(pair.server);
        let server_entity = server.world_mut().spawn(server_endpoint).id();
        server.world_mut().trigger(LinkStart {
            entity: server_entity,
        });
        let client_entity = client
            .world_mut()
            .spawn((Endpoint::from(client_parts(pair.client)), Link::default()))
            .id();
        client.world_mut().trigger(LinkStart {
            entity: client_entity,
        });
        Self {
            network,
            client,
            server,
            client_entity,
            now: Duration::ZERO,
        }
    }

    /// Runs `n` frames of each app, the client's first, on the wire time.
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.network.set_time(self.now);
            self.client.update();
            self.server.update();
            self.now += FRAME;
        }
    }

    /// The server's link to the client, once it linked.
    fn server_link(&mut self) -> Entity {
        let world = self.server.world_mut();
        let mut links = world.query_filtered::<Entity, With<LinkOf>>();
        let links: Vec<_> = links.iter(world).collect();
        assert_eq!(links.len(), 1, "the server holds one link");
        links[0]
    }

    fn send(app: &mut App, entity: Entity, token: u64, data: &[u8]) {
        app.world_mut()
            .get_mut::<Link>(entity)
            .unwrap()
            .session
            .send(PayloadToken(token), PayloadKind::new(1), payload(data));
    }
}

/// The client links at once and its first packet makes the server spawn its link, whose peer is
/// the client's address; a payload then crosses each way, and each sender learns its delivery.
/// The client has nothing to send after the pong, so it acknowledges it in an ACK-only packet
/// 25 ms after it arrived: the pong goes in the server's first frame, arrives in the next, and
/// its acknowledgement reaches the server 3.5 frames later, in the fifth.
#[test]
fn a_client_links_and_payloads_cross_both_ways() {
    let mut harness = Harness::new(DuplexModel::perfect());
    harness.frames(2);
    assert!(
        harness
            .client
            .world()
            .get::<Linked>(harness.client_entity)
            .is_some()
    );
    let link = harness.server_link();
    let server_world = harness.server.world();
    assert!(server_world.get::<Linked>(link).is_some());
    // A plain session has no TLS session to bind.
    assert!(server_world.get::<LinkBinding>(link).is_none());
    assert!(
        harness
            .client
            .world()
            .get::<LinkBinding>(harness.client_entity)
            .is_none()
    );
    assert_eq!(
        server_world.get::<PeerAddr>(link).unwrap().0,
        EmulatedNetwork::new(WireClock::Driven)
            .pair(DuplexModel::default())
            .server
            .peer_addr(),
        "the first link's client end has the first link's address"
    );

    let client = harness.client_entity;
    Harness::send(&mut harness.client, client, 1, b"ping");
    harness.frames(2);
    Harness::send(&mut harness.server, link, 2, b"pong");
    harness.frames(4);
    assert!(harness.server.world().resource::<Inbox>().fates.is_empty());
    harness.frames(1);
    let server_inbox = harness.server.world().resource::<Inbox>();
    assert_eq!(server_inbox.opened, [(link, b"ping".to_vec())]);
    assert_eq!(
        server_inbox.fates,
        [(link, PayloadFate::Delivered(PayloadToken(2)))]
    );
    let client_inbox = harness.client.world().resource::<Inbox>();
    assert_eq!(client_inbox.opened, [(client, b"pong".to_vec())]);
    assert_eq!(
        client_inbox.fates,
        [(client, PayloadFate::Delivered(PayloadToken(1)))]
    );
}

/// A conditioner of 30 ms on the server's endpoint holds what it receives: a payload the client
/// sends in a frame opens at the server 3 frames later than with none, and the client's RTT sample
/// of it, less the ACK delay, is 30 ms more.
#[test]
fn the_conditioner_delays_what_the_endpoint_receives() {
    let measure = |conditioner| {
        let config = EndpointConfig {
            conditioner,
            ..EndpointConfig::default()
        };
        let mut harness = Harness::with_server(DuplexModel::perfect(), config);
        harness.frames(10);
        let client = harness.client_entity;
        Harness::send(&mut harness.client, client, 1, b"held");
        let mut frames = 0;
        while harness.server.world().resource::<Inbox>().opened.is_empty() {
            harness.frames(1);
            frames += 1;
        }
        let link = harness.server_link();
        Harness::send(&mut harness.server, link, 2, b"reply");
        harness.frames(8);
        let samples = &harness.client.world().resource::<Inbox>().rtt_samples;
        (frames, samples.last().unwrap().1)
    };
    let (frames, sample) = measure(None);
    let held = Some(LinkConditioning::from(
        LinkModel::perfect().with_latency(Duration::from_millis(30)),
    ));
    let (held_frames, held_sample) = measure(held);
    assert_eq!(held_frames, frames + 3);
    assert_eq!(held_sample, sample + Duration::from_millis(30));
}

/// The client's sends, one a frame, under 20 % loss each way, and two runs of one seed: each
/// payload has exactly one fate, the server opened each delivered one, and the runs repeat each
/// fate and each byte.
#[test]
fn each_payload_has_one_fate_and_a_seed_repeats_exactly() {
    let run = || {
        let model = LinkModel::perfect()
            .with_latency(Duration::from_millis(15))
            .with_fixed_loss(0.2);
        let mut harness = Harness::new(DuplexModel {
            seeds: [7, 7],
            ..DuplexModel::symmetric(model)
        });
        harness.frames(3);
        let client = harness.client_entity;
        for token in 0..100 {
            Harness::send(&mut harness.client, client, token, &token.to_be_bytes());
            harness.frames(1);
        }
        harness.frames(200);
        let fates = harness.client.world().resource::<Inbox>().fates_by_token();
        let opened = harness.server.world().resource::<Inbox>().opened.clone();
        (fates, opened)
    };
    let (fates, opened) = run();
    let tokens: Vec<_> = fates
        .iter()
        .map(|fate| match fate {
            PayloadFate::Delivered(token) | PayloadFate::Lost(token) => token.0,
        })
        .collect();
    assert_eq!(tokens, (0..100).collect::<Vec<_>>());
    let delivered: Vec<_> = fates
        .iter()
        .filter_map(|fate| match fate {
            PayloadFate::Delivered(token) => Some(token.0.to_be_bytes().to_vec()),
            PayloadFate::Lost(_) => None,
        })
        .collect();
    assert!(delivered.len() < 100, "the loss lost some");
    for data in &delivered {
        assert!(opened.iter().any(|(_, opened)| opened == data));
    }
    assert_eq!(run(), (fates, opened), "two runs of one seed differ");
}

/// A server that refuses every payload acknowledges none: each is lost, by the packets the
/// client's keep-alives and the server's pings and ACKs number past it.
#[test]
fn an_unaccepted_payload_is_never_delivered() {
    let mut harness = Harness::new(DuplexModel::perfect());
    harness.server.world_mut().resource_mut::<Inbox>().refuse = true;
    harness.frames(2);
    let client = harness.client_entity;
    for token in 0..5 {
        Harness::send(&mut harness.client, client, token, b"refused");
        harness.frames(1);
    }
    harness.frames(300);
    let fates = harness.client.world().resource::<Inbox>().fates_by_token();
    assert_eq!(
        fates,
        (0..5)
            .map(|token| PayloadFate::Lost(PayloadToken(token)))
            .collect::<Vec<_>>()
    );
    assert_eq!(harness.server.world().resource::<Inbox>().opened.len(), 5);
}

/// A client's local `Unlink` with a code closes its session, and the server's link unlinks with
/// the peer's code.
#[test]
fn a_close_reaches_the_peer_with_its_code() {
    let mut harness = Harness::new(DuplexModel::perfect());
    harness.frames(2);
    let link = harness.server_link();
    let code = AppCode::new(40);
    harness.client.world_mut().trigger(Unlink {
        entity: harness.client_entity,
        reason: UnlinkReason::Closed(code),
    });
    harness.frames(2);
    assert_eq!(
        harness
            .server
            .world()
            .get::<Unlinked>(link)
            .map(|unlinked| unlinked.reason),
        Some(UnlinkReason::ClosedByPeer(code))
    );
}

/// A start of a linked client keeps its session. A client that unlinked and starts again opens a
/// new session on its endpoint: the server's last link unlinks at the close, a new one links,
/// and a payload crosses on it.
#[test]
fn a_client_connects_again_on_its_endpoint() {
    let mut harness = Harness::new(DuplexModel::perfect());
    harness.frames(2);
    let first = harness.server_link();
    let client = harness.client_entity;
    harness
        .client
        .world_mut()
        .trigger(LinkStart { entity: client });
    harness.frames(2);
    assert_eq!(harness.server_link(), first);
    assert!(harness.server.world().get::<Linked>(first).is_some());
    harness.client.world_mut().trigger(Unlink {
        entity: client,
        reason: UnlinkReason::Closed(AppCode::new(30)),
    });
    harness.frames(2);
    assert!(harness.server.world().get::<Unlinked>(first).is_some());
    harness.server.world_mut().entity_mut(first).despawn();
    harness
        .client
        .world_mut()
        .trigger(LinkStart { entity: client });
    harness.frames(2);
    assert!(harness.client.world().get::<Linked>(client).is_some());
    let second = harness.server_link();
    assert_ne!(second, first);
    Harness::send(&mut harness.client, client, 1, b"again");
    harness.frames(2);
    assert_eq!(
        harness.server.world().resource::<Inbox>().opened,
        [(second, b"again".to_vec())]
    );
}

/// On the OS clock the pass runs on the IO thread: a payload crosses, and is acknowledged, with
/// no frame driving the sessions' timers.
#[test]
fn a_real_clock_runs_the_pass_on_a_thread() {
    let network = EmulatedNetwork::new(WireClock::Os);
    let pair = network.pair(DuplexModel::default());
    let app = || {
        let mut app = App::new();
        app.insert_resource(Clock::new(ClockSource::Os));
        app.add_plugins((
            TimePlugin,
            ClockPlugin,
            LinkPlugin,
            EndpointPlugin::<EmulatedWires>::default(),
        ));
        app.init_resource::<Inbox>();
        app.add_systems(Update, Inbox::take);
        app
    };
    let mut client = app();
    let mut server = app();
    let server_endpoint = Endpoint::from(EndpointParts::plain_server(EndpointConfig::default()));
    server_endpoint.io().add(pair.server);
    let server_entity = server.world_mut().spawn(server_endpoint).id();
    server.world_mut().trigger(LinkStart {
        entity: server_entity,
    });
    let client_entity = client
        .world_mut()
        .spawn((
            Endpoint::from(EndpointParts::plain_client(
                pair.client,
                EndpointConfig::default(),
            )),
            Link::default(),
        ))
        .id();
    client.world_mut().trigger(LinkStart {
        entity: client_entity,
    });
    client.update();
    server.update();
    client
        .world_mut()
        .get_mut::<Link>(client_entity)
        .unwrap()
        .session
        .send(PayloadToken(1), PayloadKind::new(1), payload(b"threaded"));
    let start = std::time::Instant::now();
    loop {
        client.update();
        server.update();
        let delivered = client
            .world()
            .resource::<Inbox>()
            .fates
            .contains(&(client_entity, PayloadFate::Delivered(PayloadToken(1))));
        if delivered {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "the payload crossed within 500 ms"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let server_world = server.world_mut();
    let opened = &server_world.resource::<Inbox>().opened;
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].1, b"threaded");
}

/// The binding every session of [`Bound`] carries.
const BINDING: ChannelBinding = ChannelBinding([7; 32]);

/// Plain sessions that each carry [`BINDING`], as a sealed establishment's carry their TLS
/// session's: a client's opens at its connect, and a server takes each peer by its first datagram.
#[derive(Debug, Default)]
struct Bound {
    accepting: bool,
}

impl Bound {
    const fn established(peer: SocketAddr) -> Established {
        Established {
            peer,
            protection: Protection::Plain,
            prefix: None,
            route: None,
            reset_token: None,
            binding: Some(BINDING),
        }
    }
}

impl Establish for Bound {
    fn receive(&mut self, _: Instant, _: Datagram, _: &mut Establishing) {}

    fn poll(&mut self, _: Instant, _: &mut Establishing) {}

    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    fn routes_by_prefix(&self) -> bool {
        false
    }

    fn challenge(&mut self) -> [u8; 8] {
        unreachable!("a session routed by address has no candidate path")
    }

    fn ended(&mut self, _: Option<ConnectionId>) {}
}

impl ClientEstablish for Bound {
    fn connect(&mut self, _: Instant, server: SocketAddr, out: &mut Establishing) {
        out.established.push(Self::established(server));
    }
}

impl ServerEstablish for Bound {
    fn unknown(&mut self, _: Instant, datagram: &Datagram, out: &mut Establishing) {
        if self.accepting {
            out.established.push(Self::established(datagram.from));
        }
    }

    fn set_accepting(&mut self, accepting: bool) {
        self.accepting = accepting;
    }
}

/// A session's binding goes on the entity that carries it, as `LinkBinding`: the client's own and
/// the server's link hold the same one.
#[test]
fn a_bound_session_puts_its_binding_on_its_link() {
    let mut harness = Harness::with_endpoints(
        DuplexModel::perfect(),
        EndpointParts::server(
            EmulatedWires::server(),
            Box::new(Bound::default()),
            EndpointConfig::default(),
        ),
        |wire| {
            let server = wire.peer_addr();
            EndpointParts::client(
                EmulatedWires::client(wire),
                server,
                Box::new(Bound::default()),
                EndpointConfig::default(),
            )
        },
    );
    harness.frames(2);
    let link = harness.server_link();
    assert_eq!(
        harness.server.world().get::<LinkBinding>(link),
        Some(&LinkBinding(BINDING))
    );
    assert_eq!(
        harness
            .client
            .world()
            .get::<LinkBinding>(harness.client_entity),
        Some(&LinkBinding(BINDING))
    );
}
