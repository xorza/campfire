use std::io::ErrorKind;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use bevy_app::{App, TaskPoolPlugin, Update};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::ResMut;
use bevy_ecs::world::{Mut, World};
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use bytes::{Bytes, BytesMut};
use lightyear::link::{LinkStart, RecvPayload, Unlink};
use lightyear::prelude::client::{ClientPlugins, RawClient};
use lightyear::prelude::server::{RawServer, ServerPlugins, Start};
use lightyear::prelude::{
    AppChannelExt, AppMessageExt, ChannelMode, ChannelSettings, Client, Connect, Connected,
    MessageReceiver, MessageSender, NetworkDirection, PredictionManager, ReliableSettings,
};
use lightyear::transport::packet::fragment_size_for_min_mtu;
use lightyear_serde::SerializationError;
use lightyear_serde::reader::Reader;
use lightyear_serde::registry::SerializeFns;
use lightyear_serde::writer::Writer;
use quinn_proto::{Connection, DatagramEvent, Dir, Event};

use super::*;
use crate::QuicPlugin;
use crate::close_link::CloseLink;
use crate::link_binding::LinkBinding;

/// The longest a test waits for the network on loopback, inside a test's bound of 1 s.
const PATIENCE: Duration = Duration::from_millis(800);
/// Times short enough that an idle timeout passes inside a test: 4 keep-alives to it.
const SHORT: LinkTimes = LinkTimes {
    keep_alive: Duration::from_millis(50),
    idle_timeout: Duration::from_millis(200),
};

/// `len` bytes counting modulo 251, a prime, so no shift by a power of two, as a fragment put in
/// the wrong place, leaves them the same.
fn pattern(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| u8::try_from(index % 251).unwrap())
        .collect()
}

/// The secret of the test server's own key, from which its TLS key and reset key derive.
fn server_secret() -> Zeroizing<[u8; 32]> {
    Zeroizing::new([7; 32])
}

fn server_key() -> ServerKey {
    ServerKey::derive(&server_secret())
}

/// A server's app on loopback with its endpoint linked, which clients join.
#[derive(Debug)]
struct Loopback {
    server: App,
    endpoint: Entity,
    address: SocketAddr,
    key: TlsKey,
}

/// A client's app and the entity of its link and endpoint.
#[derive(Debug)]
struct ClientApp {
    app: App,
    entity: Entity,
}

impl Loopback {
    fn new(times: LinkTimes) -> Loopback {
        let mut loopback = Loopback::bind(times);
        loopback.start();
        loopback
    }

    /// The server's endpoint on a port of the OS's choice, bound and not started.
    fn bind(times: LinkTimes) -> Loopback {
        Loopback::bind_at(SocketAddr::from(([127, 0, 0, 1], 0)), times)
    }

    /// The server's endpoint on `address`, bound and not started, its key and its reset key the
    /// same at each bind.
    fn bind_at(address: SocketAddr, times: LinkTimes) -> Loopback {
        let quic = QuicEndpoint::server(address, &server_secret(), times).unwrap();
        let address = quic.local_address();
        let key = quic.tls_key();
        let mut server = App::new();
        server.add_plugins(QuicPlugin);
        let endpoint = server.world_mut().spawn(quic).id();
        Loopback {
            server,
            endpoint,
            address,
            key,
        }
    }

    fn start(&mut self) {
        let entity = self.endpoint;
        self.server.world_mut().trigger(LinkStart { entity });
        self.server.update();
    }

    fn client(&self, times: LinkTimes) -> ClientApp {
        ClientApp::start(QuicEndpoint::client(self.address, &self.key, times).unwrap())
    }

    /// Updates the server and `clients` until `done`, a frame each millisecond: how long it took.
    fn pump(
        &mut self,
        clients: &mut [&mut ClientApp],
        done: impl Fn(&World, &[&mut ClientApp]) -> bool,
    ) -> Duration {
        let start = Instant::now();
        loop {
            self.server.update();
            for client in clients.iter_mut() {
                client.app.update();
            }
            if done(self.server.world(), clients) {
                return start.elapsed();
            }
            assert!(start.elapsed() < PATIENCE, "the network did not answer");
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Updates until `client` and the server are both linked: the server's link.
    fn join(&mut self, client: &mut ClientApp) -> Entity {
        self.pump(&mut [client], |server, clients| {
            clients[0].is::<Linked>()
                && server
                    .iter_entities()
                    .any(|link| link.contains::<LinkOf>() && link.contains::<Linked>())
        });
        Loopback::links(self.server.world_mut())[0]
    }

    /// The server's linked links.
    fn links(world: &mut World) -> Vec<Entity> {
        world
            .query_filtered::<Entity, (With<LinkOf>, With<Linked>)>()
            .iter(world)
            .collect()
    }

    fn quic(&self) -> &QuicEndpoint {
        self.server
            .world()
            .get::<QuicEndpoint>(self.endpoint)
            .unwrap()
    }
}

impl ClientApp {
    fn start(quic: QuicEndpoint) -> ClientApp {
        let mut app = App::new();
        app.add_plugins(QuicPlugin);
        let entity = app
            .world_mut()
            .spawn((Link::default().with_mtu(LinkMtu::new(LINK_MIN_MTU)), quic))
            .id();
        app.world_mut().trigger(LinkStart { entity });
        ClientApp { app, entity }
    }

    fn is<C: Component>(&self) -> bool {
        self.app.world().get::<C>(self.entity).is_some()
    }

    fn link(&mut self) -> Mut<'_, Link> {
        self.app.world_mut().get_mut::<Link>(self.entity).unwrap()
    }

    fn quic(&self) -> &QuicEndpoint {
        self.app.world().get::<QuicEndpoint>(self.entity).unwrap()
    }

    /// Ends the link and opens a new one from the same socket.
    fn restart(&mut self) {
        let entity = self.entity;
        self.app.world_mut().trigger(Unlink {
            entity,
            reason: UnlinkReason::UserRequested(None),
        });
        self.app.update();
        self.app.world_mut().trigger(LinkStart { entity });
    }
}

#[test]
fn a_link_carries_datagrams_of_its_minimum_mtu_both_ways_at_once() {
    let mut loopback = Loopback::new(LinkTimes::CAMPFIRE);
    // An empty UDP datagram, which any host may send, is no QUIC packet, and the server's thread
    // reads past it.
    let stranger = UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
    stranger.send_to(&[], loopback.address).unwrap();
    let mut client = loopback.client(LinkTimes::CAMPFIRE);
    let link = loopback.join(&mut client);
    // The server presents the key its secret derives, and the client pinned that key (D6).
    assert_eq!(loopback.quic().tls_key(), server_key().public());
    assert_eq!(client.quic().tls_key(), loopback.quic().tls_key());
    // The connection took one Retry before it was accepted (D8), and its entity names its peer (D9).
    assert_eq!(loopback.quic().state().retries(), 1);
    assert_eq!(
        loopback.server.world().get::<RemoteId>(link),
        Some(&RemoteId(PeerId::Entity(link.to_bits())))
    );
    // Both ends read one TLS session's binding (D11).
    let server_binding = loopback.server.world().get::<LinkBinding>(link);
    assert!(server_binding.is_some());
    assert_eq!(
        server_binding,
        client.app.world().get::<LinkBinding>(client.entity)
    );
    // Neither end may open a stream: each gave the other a limit of none (D4).
    for quic in [loopback.quic(), client.quic()] {
        let mut state = quic.state();
        let (_, peer) = state.linked().next().unwrap();
        assert!(peer.connection.streams().open(Dir::Bi).is_none());
        assert!(peer.connection.streams().open(Dir::Uni).is_none());
    }

    // Once the handshake's acknowledgments settle, no deadline falls before the keep-alive of 1 s,
    // so a datagram that arrives well before it went out on the frame's wake (D3).
    thread::sleep(Duration::from_millis(50));
    loopback.pump(&mut [&mut client], |_, _| true);
    let up = pattern(LINK_MIN_MTU);
    client.link().send(Bytes::from(up.clone()));
    let took = loopback.pump(&mut [&mut client], |server, _| {
        server.get::<Link>(link).unwrap().recv.len() == 1
    });
    assert!(
        took < Duration::from_millis(250),
        "the thread waited {took:?}"
    );
    let mut server_link = loopback.server.world_mut().get_mut::<Link>(link).unwrap();
    // The link holds the slice of the packet `quinn-proto` gives, so no datagram is copied.
    let RecvPayload::Frozen(payload) = server_link.recv.pop().unwrap() else {
        panic!("the server's link copied its datagram");
    };
    assert_eq!(payload, up);
    let down = vec![0xa5; LINK_MIN_MTU];
    server_link.send(Bytes::from(down.clone()));
    loopback.pump(&mut [&mut client], |_, clients| {
        clients[0]
            .app
            .world()
            .get::<Link>(clients[0].entity)
            .unwrap()
            .recv
            .len()
            == 1
    });
    let RecvPayload::Frozen(payload) = client.link().recv.pop().unwrap() else {
        panic!("the client's link copied its datagram");
    };
    assert_eq!(payload, down);
}

/// Drives a handshake between a client whose connection ids are `client_cid` bytes long and a
/// server, in memory, until the server's completes: before either end raised its MTU by discovery,
/// as a probe goes out only once a handshake completed and counts once its acknowledgment returns.
/// The client's connection, then the server's.
fn handshake(client_cid: usize) -> (Connection, Connection) {
    let key = server_key();
    let now = Instant::now();
    let mut server = Endpoint::new(
        Arc::new(QuicEndpoint::endpoint_config(ResetKey::random(), CID_LEN)),
        Some(Arc::new(QuicEndpoint::server_config(
            key.server_config(),
            LinkTimes::CAMPFIRE,
        ))),
        true,
        None,
    );
    let mut client = Endpoint::new(
        Arc::new(QuicEndpoint::endpoint_config(
            ResetKey::random(),
            client_cid,
        )),
        None,
        true,
        None,
    );
    let server_address = SocketAddr::from(([10, 0, 0, 1], 4433));
    let client_address = SocketAddr::from(([10, 0, 0, 2], 50_000));
    let config = QuicEndpoint::client_config(
        PinnedKeyVerifier::new(&key.public()).client_config(),
        LinkTimes::CAMPFIRE,
    );
    let (_, mut client_connection) = client
        .connect(now, config, server_address, "campfire")
        .unwrap();
    let mut server_connection: Option<Connection> = None;
    let mut sent = Vec::new();
    let mut response = Vec::new();
    for _ in 0..8 {
        loop {
            sent.clear();
            let Some(transmit) = client_connection.poll_transmit(now, 1, &mut sent) else {
                break;
            };
            response.clear();
            let datagram = BytesMut::from(&sent[..transmit.size]);
            match server.handle(now, client_address, None, None, datagram, &mut response) {
                Some(DatagramEvent::NewConnection(incoming))
                    if incoming.remote_address_validated() =>
                {
                    let (_, connection) =
                        server.accept(incoming, now, &mut response, None).unwrap();
                    server_connection = Some(connection);
                }
                Some(DatagramEvent::NewConnection(incoming)) => {
                    let retry = server.retry(incoming, &mut response).unwrap();
                    let datagram = BytesMut::from(&response[..retry.size]);
                    let mut ignored = Vec::new();
                    if let Some(DatagramEvent::ConnectionEvent(_, event)) =
                        client.handle(now, server_address, None, None, datagram, &mut ignored)
                    {
                        client_connection.handle_event(event);
                    }
                }
                Some(DatagramEvent::ConnectionEvent(_, event)) => {
                    server_connection.as_mut().unwrap().handle_event(event);
                }
                Some(DatagramEvent::Response(_)) | None => {}
            }
        }
        if let Some(connection) = &mut server_connection {
            while let Some(event) = connection.poll() {
                if matches!(event, Event::Connected) {
                    return (client_connection, server_connection.unwrap());
                }
            }
            loop {
                sent.clear();
                let Some(transmit) = connection.poll_transmit(now, 1, &mut sent) else {
                    break;
                };
                let datagram = BytesMut::from(&sent[..transmit.size]);
                if let Some(DatagramEvent::ConnectionEvent(_, event)) =
                    client.handle(now, server_address, None, None, datagram, &mut response)
                {
                    client_connection.handle_event(event);
                }
            }
        }
        while client_connection.poll().is_some() {}
    }
    panic!("the handshake did not complete");
}

#[test]
fn the_link_minimum_is_quics_datagram_on_its_minimum_path() {
    // Each end's largest datagram counts the ids the other end gives: 1200 − (1 + 8 + 4 + 16) − 9
    // = 1162 with Campfire's 8 bytes, and 12 fewer, 1150, for a client that gives 20, QUIC's
    // longest, which a server then cannot link (D5).
    for (client_cid, server_max) in [(CID_LEN, LINK_MIN_MTU), (20, 1150)] {
        let (mut client, mut server) = handshake(client_cid);
        assert_eq!(client.datagrams().max_size(), Some(LINK_MIN_MTU));
        assert_eq!(server.datagrams().max_size(), Some(server_max));
    }
}

#[test]
fn a_client_whose_datagrams_cannot_carry_a_link_is_closed_and_never_linked() {
    let mut loopback = Loopback::new(LinkTimes::CAMPFIRE);
    let config = QuicEndpoint::client_config(
        PinnedKeyVerifier::new(&loopback.key).client_config(),
        LinkTimes::CAMPFIRE,
    );
    let mut client = ClientApp::start(
        QuicEndpoint::client_of(loopback.address, loopback.key, config, 20).unwrap(),
    );
    loopback.pump(&mut [&mut client], |_, clients| clients[0].is::<Unlinked>());
    assert!(Loopback::links(loopback.server.world_mut()).is_empty());
    assert_eq!(
        client.app.world().get::<LinkEnd>(client.entity),
        Some(&LinkEnd::Unfit)
    );
    // The client's handshake completed before the server closed it: it spawned no link either.
    assert!(Loopback::links(client.app.world_mut()).is_empty());
}

#[test]
fn a_port_in_use_is_an_error_that_names_its_address() {
    let first = Loopback::bind(LinkTimes::CAMPFIRE);
    let taken = first.address;
    let refused = QuicEndpoint::server(taken, &server_secret(), LinkTimes::CAMPFIRE).unwrap_err();
    assert_eq!(refused.address, taken);
    assert_eq!(refused.error.kind(), ErrorKind::AddrInUse);
}

#[test]
fn a_server_takes_connections_only_while_started() {
    let refused = |client: &ClientApp| {
        client.app.world().get::<LinkEnd>(client.entity) == Some(&LinkEnd::Refused)
    };
    // Bound, not started: a client is refused.
    let mut loopback = Loopback::bind(LinkTimes::CAMPFIRE);
    let mut client = loopback.client(LinkTimes::CAMPFIRE);
    loopback.pump(&mut [&mut client], |_, clients| clients[0].is::<Unlinked>());
    assert!(refused(&client));
    // Started: it links, and its last end clears.
    loopback.start();
    client.restart();
    loopback.join(&mut client);
    assert!(!client.is::<LinkEnd>());
    // Ended: its links end, and a new client is refused.
    let entity = loopback.endpoint;
    loopback.server.world_mut().trigger(Unlink {
        entity,
        reason: UnlinkReason::ServerStopped,
    });
    let mut late = loopback.client(LinkTimes::CAMPFIRE);
    loopback.pump(&mut [&mut client, &mut late], |_, clients| {
        clients[0].is::<Unlinked>() && clients[1].is::<Unlinked>()
    });
    assert!(refused(&late));
    assert!(Loopback::links(loopback.server.world_mut()).is_empty());
}

#[test]
fn a_connection_a_server_ends_before_its_frame_is_never_linked() {
    let mut loopback = Loopback::new(LinkTimes::CAMPFIRE);
    let mut client = loopback.client(LinkTimes::CAMPFIRE);
    // The server's thread completes the handshake while its app runs no frame, then the server
    // ends before the frame that would link it: the connection closes as its links would.
    let start = Instant::now();
    while !client.is::<Linked>() {
        assert!(start.elapsed() < PATIENCE, "the handshake did not complete");
        client.app.update();
        thread::sleep(Duration::from_millis(1));
    }
    let entity = loopback.endpoint;
    loopback.server.world_mut().trigger(Unlink {
        entity,
        reason: UnlinkReason::ServerStopped,
    });
    loopback.pump(&mut [&mut client], |_, clients| clients[0].is::<Unlinked>());
    assert!(Loopback::links(loopback.server.world_mut()).is_empty());
    assert_eq!(
        client.app.world().get::<LinkEnd>(client.entity),
        Some(&LinkEnd::Ended)
    );
}

#[test]
fn two_connections_from_one_socket_are_two_peers_and_the_second_skips_the_retry_by_a_token() {
    let mut loopback = Loopback::new(LinkTimes::CAMPFIRE);
    let mut client = loopback.client(LinkTimes::CAMPFIRE);
    loopback.join(&mut client);
    // The server's tokens may come after the client linked.
    loopback.pump(&mut [&mut client], |_, clients| {
        let mut state = clients[0].quic().state();
        let (_, peer) = state.linked().next().unwrap();
        peer.connection.stats().frame_rx.new_token >= u64::from(TOKENS_SENT)
    });
    // The client drops its connection with no close, as one that lost its state, and connects
    // again from its socket while the server still holds the first.
    client.quic().state().forget(client.entity);
    client.restart();
    loopback.pump(&mut [&mut client], |server, clients| {
        clients[0].is::<Linked>()
            && server
                .iter_entities()
                .filter(|link| link.contains::<LinkOf>() && link.contains::<Linked>())
                .count()
                == 2
    });
    let links = Loopback::links(loopback.server.world_mut());
    let world = loopback.server.world();
    let [first, second] = [links[0], links[1]].map(|link| world.get::<PeerAddr>(link).unwrap().0);
    assert_eq!(first, second);
    let [first, second] = [links[0], links[1]].map(|link| *world.get::<RemoteId>(link).unwrap());
    assert_ne!(first, second);
    // Two TLS sessions, two bindings: the client's is its second session's.
    let [first, second] = [links[0], links[1]].map(|link| *world.get::<LinkBinding>(link).unwrap());
    assert_ne!(first, second);
    let client_binding = *client
        .app
        .world()
        .get::<LinkBinding>(client.entity)
        .unwrap();
    assert!(client_binding == first || client_binding == second);
    assert_ne!(first == client_binding, second == client_binding);
    // The second connection's token validated its address: only the first took a Retry (D8).
    assert_eq!(loopback.quic().state().retries(), 1);
}

#[test]
fn a_loss_the_frame_has_not_taken_ends_no_later_connection() {
    let mut loopback = Loopback::new(LinkTimes::CAMPFIRE);
    let mut client = loopback.client(LinkTimes::CAMPFIRE);
    let link = loopback.join(&mut client);
    // The server closes the link, and the client's thread holds its loss while the client's app
    // runs no frame.
    let code = AppCode::new(17);
    loopback
        .server
        .world_mut()
        .trigger(CloseLink { entity: link, code });
    loopback.server.update();
    let start = Instant::now();
    while !client.quic().state().has_news() {
        assert!(start.elapsed() < PATIENCE, "the close did not arrive");
        thread::sleep(Duration::from_millis(1));
    }
    // In one frame the client ends its link and opens another: the loss of the first is not the
    // second's.
    let entity = client.entity;
    client.app.world_mut().trigger(Unlink {
        entity,
        reason: UnlinkReason::UserRequested(None),
    });
    client.app.world_mut().flush();
    client.app.world_mut().trigger(LinkStart { entity });
    loopback.pump(&mut [&mut client], |_, clients| clients[0].is::<Linked>());
    assert!(!client.is::<LinkEnd>());
}

#[test]
fn a_wrong_pin_fails_by_access_denied_and_each_connect_checks_the_key() {
    let mut loopback = Loopback::new(LinkTimes::CAMPFIRE);
    let wrong = ServerKey::from_seed(&Zeroizing::new([8; 32])).public();
    let mut impostor = ClientApp::start(
        QuicEndpoint::client(loopback.address, &wrong, LinkTimes::CAMPFIRE).unwrap(),
    );
    loopback.pump(&mut [&mut impostor], |_, clients| {
        clients[0].is::<Unlinked>()
    });
    assert_eq!(
        impostor.app.world().get::<LinkEnd>(impostor.entity),
        Some(&LinkEnd::WrongServerKey)
    );
    // The server's handshake failed by the client's alert: it spawned no link.
    loopback.pump(&mut [], |_, _| true);
    assert!(Loopback::links(loopback.server.world_mut()).is_empty());

    // With no session ticket, a second connect presents the key again, and the verifier reads it.
    let checks = Arc::new(AtomicUsize::new(0));
    let config = QuicEndpoint::client_config(
        PinnedKeyVerifier::new(&loopback.key).counting(&checks),
        LinkTimes::CAMPFIRE,
    );
    let mut client = ClientApp::start(
        QuicEndpoint::client_of(loopback.address, loopback.key, config, CID_LEN).unwrap(),
    );
    loopback.pump(&mut [&mut client], |_, clients| clients[0].is::<Linked>());
    client.restart();
    loopback.pump(&mut [&mut client], |_, clients| clients[0].is::<Linked>());
    assert_eq!(checks.load(Ordering::Relaxed), 2);
}

/// One way a link ends: what it does, and the `LinkEnd` the server's link and the client then
/// hold. A QUIC link that its own end closed holds its code, `ENDED` for a plain `Unlink`; a
/// despawned link holds nothing.
#[derive(Debug)]
struct Ending {
    act: fn(&mut Loopback, &mut ClientApp),
    at_server: Option<LinkEnd>,
    at_client: Option<LinkEnd>,
}

impl Ending {
    /// Every way a link ends that the other end hears at once.
    fn all() -> Vec<Ending> {
        let mut ends = Ending::by_client();
        ends.extend(Ending::by_server());
        ends
    }

    fn by_client() -> Vec<Ending> {
        vec![
            // The client unlinks, with no reason.
            Ending {
                act: |_, client| {
                    let entity = client.entity;
                    client.app.world_mut().trigger(Unlink {
                        entity,
                        reason: UnlinkReason::UserRequested(None),
                    });
                },
                at_server: Some(LinkEnd::Ended),
                at_client: Some(LinkEnd::Closed(AppCode::ENDED)),
            },
            // The client closes with its app's code.
            Ending {
                act: |_, client| {
                    let entity = client.entity;
                    let code = AppCode::new(17);
                    client.app.world_mut().trigger(CloseLink { entity, code });
                },
                at_server: Some(LinkEnd::ClosedByPeer(AppCode::new(17))),
                at_client: Some(LinkEnd::Closed(AppCode::new(17))),
            },
            // The client's endpoint leaves its entity: removed, or replaced by another.
            Ending {
                act: |_, client| {
                    client
                        .app
                        .world_mut()
                        .entity_mut(client.entity)
                        .remove::<QuicEndpoint>();
                },
                at_server: Some(LinkEnd::PeerStopped),
                at_client: Some(LinkEnd::Closed(AppCode::STOPPED)),
            },
            Ending {
                act: |loopback, client| {
                    let quic =
                        QuicEndpoint::client(loopback.address, &loopback.key, LinkTimes::CAMPFIRE)
                            .unwrap();
                    client
                        .app
                        .world_mut()
                        .entity_mut(client.entity)
                        .insert(quic);
                },
                at_server: Some(LinkEnd::PeerStopped),
                at_client: Some(LinkEnd::Closed(AppCode::STOPPED)),
            },
        ]
    }

    fn by_server() -> Vec<Ending> {
        vec![
            // The server despawns its link.
            Ending {
                act: |loopback, _| {
                    let link = Loopback::links(loopback.server.world_mut())[0];
                    loopback.server.world_mut().despawn(link);
                },
                at_server: None,
                at_client: Some(LinkEnd::Ended),
            },
            // The server closes its link with its app's code.
            Ending {
                act: |loopback, _| {
                    let entity = Loopback::links(loopback.server.world_mut())[0];
                    let code = AppCode::new(18);
                    loopback
                        .server
                        .world_mut()
                        .trigger(CloseLink { entity, code });
                },
                at_server: Some(LinkEnd::Closed(AppCode::new(18))),
                at_client: Some(LinkEnd::ClosedByPeer(AppCode::new(18))),
            },
            // The server ends, which despawns its links.
            Ending {
                act: |loopback, _| {
                    let entity = loopback.endpoint;
                    loopback.server.world_mut().trigger(Unlink {
                        entity,
                        reason: UnlinkReason::ServerStopped,
                    });
                },
                at_server: None,
                at_client: Some(LinkEnd::Ended),
            },
            // The server's endpoint leaves its entity, which unlinks and despawns its links, or goes
            // with its entity, which despawns them too.
            Ending {
                act: |loopback, _| {
                    let entity = loopback.endpoint;
                    loopback
                        .server
                        .world_mut()
                        .entity_mut(entity)
                        .remove::<QuicEndpoint>();
                },
                at_server: None,
                at_client: Some(LinkEnd::PeerStopped),
            },
            Ending {
                act: |loopback, _| {
                    let entity = loopback.endpoint;
                    loopback.server.world_mut().despawn(entity);
                },
                at_server: None,
                at_client: Some(LinkEnd::PeerStopped),
            },
            // The server's app exits, and drops its world.
            Ending {
                act: |loopback, _| loopback.server = App::new(),
                at_server: None,
                at_client: Some(LinkEnd::PeerStopped),
            },
        ]
    }
}

#[test]
fn each_end_of_a_link_reaches_the_other_at_once_with_its_cause() {
    // Each end reaches the other inside a test's second, where the idle timeout is 5 s: by the
    // close the ending side sends, which carries why (D10).
    for ending in Ending::all() {
        let mut loopback = Loopback::new(LinkTimes::CAMPFIRE);
        let mut client = loopback.client(LinkTimes::CAMPFIRE);
        let link = loopback.join(&mut client);
        (ending.act)(&mut loopback, &mut client);
        loopback.pump(&mut [&mut client], |server, clients| {
            let server_ended =
                server.get_entity(link).is_err() || server.get::<Unlinked>(link).is_some();
            server_ended && clients[0].is::<Unlinked>()
        });
        let world = loopback.server.world();
        let at_server = world
            .get_entity(link)
            .ok()
            .and_then(|link| link.get::<LinkEnd>());
        assert_eq!(at_server, ending.at_server.as_ref(), "{ending:?}");
        let at_client = client.app.world().get::<LinkEnd>(client.entity);
        assert_eq!(at_client, ending.at_client.as_ref(), "{ending:?}");
        // A server's entity that holds its endpoint ends with it.
        if let Ok(server) = world.get_entity(loopback.endpoint)
            && !server.contains::<QuicEndpoint>()
        {
            assert!(server.contains::<Unlinked>(), "{ending:?}");
        }
    }
}

#[test]
fn a_server_started_again_on_its_reset_key_resets_its_old_connections() {
    let mut loopback = Loopback::new(LinkTimes::CAMPFIRE);
    let mut client = loopback.client(LinkTimes::CAMPFIRE);
    let link = loopback.join(&mut client);
    // The server stops with no close, as a crash stops it, and starts again on its port and its
    // reset key: the client's next packet meets a reset, not the idle timeout (D8).
    loopback.quic().state().forget(link);
    let address = loopback.address;
    drop(loopback);
    let mut loopback = Loopback::bind_at(address, LinkTimes::CAMPFIRE);
    loopback.start();
    client
        .link()
        .send(Bytes::from_static(b"to the server that stopped"));
    loopback.pump(&mut [&mut client], |_, clients| clients[0].is::<Unlinked>());
    assert_eq!(
        client.app.world().get::<LinkEnd>(client.entity),
        Some(&LinkEnd::Reset)
    );
}

#[test]
fn a_silent_peer_is_lost_by_the_idle_timeout_and_a_stalled_app_is_not() {
    let mut loopback = Loopback::new(SHORT);
    let mut client = loopback.client(SHORT);
    let link = loopback.join(&mut client);
    // Neither app runs a frame for twice the idle timeout; their threads keep the link (D3).
    thread::sleep(SHORT.idle_timeout * 2);
    client.link().send(Bytes::from_static(b"after the stall"));
    loopback.pump(&mut [&mut client], |server, _| {
        server.get::<Link>(link).unwrap().recv.len() == 1
    });
    assert!(client.is::<Linked>());

    // A client that drops its connection sends nothing more: the server loses it by the idle
    // timeout, and not before its last keep-alive's.
    client.quic().state().forget(client.entity);
    let took = loopback.pump(&mut [], |server, _| server.get::<Unlinked>(link).is_some());
    assert!(
        took >= SHORT.idle_timeout.checked_sub(SHORT.keep_alive).unwrap(),
        "lost after {took:?}"
    );
    assert_eq!(
        loopback.server.world().get::<LinkEnd>(link),
        Some(&LinkEnd::TimedOut)
    );
}

/// The bytes of one message a test's link carries, behind their length.
#[derive(Debug, PartialEq, Eq)]
struct Blob(Vec<u8>);

#[derive(Debug)]
struct BlobChannel;

/// The blobs the server's links received, which Lightyear would clear at the frame's end.
#[derive(Resource, Debug, Default)]
struct Received(Vec<Blob>);

impl Blob {
    #[expect(
        clippy::unnecessary_wraps,
        reason = "Lightyear's function type returns a Result"
    )]
    fn write(blob: &Blob, writer: &mut Writer) -> Result<(), SerializationError> {
        let len = u32::try_from(blob.0.len()).unwrap();
        writer.extend_from_slice(&len.to_le_bytes());
        writer.extend_from_slice(&blob.0);
        Ok(())
    }

    fn read(reader: &mut Reader) -> Result<Blob, SerializationError> {
        let (len, rest) = reader
            .remaining_slice()
            .split_first_chunk::<4>()
            .ok_or(SerializationError::InvalidValue)?;
        let len = usize::try_from(u32::from_le_bytes(*len)).unwrap();
        let blob = Blob(
            rest.get(..len)
                .ok_or(SerializationError::InvalidValue)?
                .to_vec(),
        );
        reader.set_position(reader.position() + 4 + u64::try_from(len).unwrap());
        Ok(blob)
    }

    fn register(app: &mut App) {
        app.add_channel::<BlobChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..ChannelSettings::default()
        })
        .add_direction(NetworkDirection::ClientToServer);
        app.register_message_custom_serde::<Blob>(SerializeFns {
            serialize: Blob::write,
            deserialize: Blob::read,
        })
        .add_direction(NetworkDirection::ClientToServer);
    }

    fn receive(
        mut links: Query<'_, '_, &mut MessageReceiver<Blob>>,
        mut received: ResMut<'_, Received>,
    ) {
        for mut link in &mut links {
            received.0.extend(link.receive());
        }
    }
}

#[test]
fn a_message_of_three_fragments_crosses_lightyear_whole() {
    let tick = Duration::from_millis(16);
    let quic = QuicEndpoint::server(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        &server_secret(),
        LinkTimes::CAMPFIRE,
    )
    .unwrap();
    let address = quic.local_address();
    let key = quic.tls_key();
    let mut server = App::new();
    server.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
    server.add_plugins(ServerPlugins {
        tick_duration: tick,
    });
    server.add_plugins(QuicPlugin);
    Blob::register(&mut server);
    server.init_resource::<Received>();
    server.add_systems(Update, Blob::receive);
    server.finish();
    server.cleanup();
    let endpoint = server.world_mut().spawn((RawServer, quic)).id();
    server.world_mut().trigger(Start { entity: endpoint });

    let mut client = App::new();
    client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
    client.add_plugins(ClientPlugins {
        tick_duration: tick,
    });
    client.add_plugins(QuicPlugin);
    client.insert_resource(PredictionManager::default());
    Blob::register(&mut client);
    client.finish();
    client.cleanup();
    let quic = QuicEndpoint::client(address, &key, LinkTimes::CAMPFIRE).unwrap();
    let entity = client
        .world_mut()
        .spawn((
            Client,
            RawClient,
            Link::default().with_mtu(LinkMtu::new(LINK_MIN_MTU)),
            quic,
        ))
        .id();
    client.world_mut().trigger(Connect { entity });

    let start = Instant::now();
    let link = loop {
        server.update();
        client.update();
        let mut links = server
            .world_mut()
            .query_filtered::<Entity, (With<LinkOf>, With<Connected>)>();
        if let Some(link) = links.iter(server.world()).next()
            && client.world().get::<Connected>(entity).is_some()
        {
            break link;
        }
        assert!(start.elapsed() < PATIENCE, "the link did not connect");
        thread::sleep(Duration::from_millis(1));
    };
    // Lightyear's raw server kept the peer id the IO gave the link (D9).
    assert_eq!(
        server.world().get::<RemoteId>(link),
        Some(&RemoteId(PeerId::Entity(link.to_bits())))
    );
    // One byte past two fragments of the link's minimum MTU: three fragments.
    let fragment = fragment_size_for_min_mtu(LINK_MIN_MTU).unwrap();
    let message = pattern(2 * fragment + 1);
    client
        .world_mut()
        .get_mut::<MessageSender<Blob>>(entity)
        .unwrap()
        .send::<BlobChannel>(Blob(message.clone()));
    let start = Instant::now();
    let received = loop {
        server.update();
        client.update();
        if let Some(received) = server.world_mut().resource_mut::<Received>().0.pop() {
            break received;
        }
        assert!(start.elapsed() < PATIENCE, "the message did not arrive");
        thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(received, Blob(message));
}
