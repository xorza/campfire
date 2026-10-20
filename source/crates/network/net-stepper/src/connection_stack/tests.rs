use super::*;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use campfire_net_connection::RemoteId;
use campfire_net_connection::{
    Client, ClientOf, Connect, Connected, Disconnected, DisconnectedReason, Start,
};
use campfire_net_core::PeerId;
use campfire_net_emulator::{DuplexModel, EmulatedNetwork, WireClock};
use campfire_net_endpoint::Endpoint;
use campfire_net_link::{Link, LinkOf, Linked, RemoteAddress, Server};
use campfire_net_packets::{EndpointPieces, EndpointSetup};
use campfire_net_payload::{AppCode, UnlinkReason};
use campfire_net_timeline::{Clock, ClockSource};
use core::time::Duration;

/// One app with a server's plain endpoint and its clients', stepped by hand on the
/// network's wire time.
struct Harness {
    app: App,
    network: EmulatedNetwork,
    server: Entity,
    now: Duration,
}

impl Harness {
    const FRAME: Duration = Duration::from_millis(10);

    fn new() -> Self {
        let mut app = App::new();
        app.insert_resource(Clock::new(ClockSource::Frame));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Self::FRAME));
        app.add_plugins((
            TimePlugin,
            ClockPlugin,
            LinkPlugin,
            EndpointPlugin::<EmulatedWires>::default(),
            ConnectionPlugins,
            ServerConnectionPlugins,
        ));
        let server = app
            .world_mut()
            .spawn((
                Server::default(),
                Endpoint::from(EndpointPieces::plain_server(EndpointSetup::default())),
            ))
            .id();
        app.world_mut().trigger(Start { entity: server });
        Self {
            app,
            network: EmulatedNetwork::new(WireClock::Driven),
            server,
            now: Duration::ZERO,
        }
    }

    /// A client on a wire of its own to the server, which connects.
    fn connect_client(&mut self) -> Entity {
        let pair = self.network.pair(DuplexModel::default());
        self.app
            .world()
            .get::<Endpoint<EmulatedWires>>(self.server)
            .unwrap()
            .io()
            .add(pair.server);
        let client = self
            .app
            .world_mut()
            .spawn((
                Client,
                Link::default(),
                Endpoint::from(EndpointPieces::plain_client(
                    pair.client,
                    EndpointSetup::default(),
                )),
            ))
            .id();
        self.app.world_mut().trigger(Connect { entity: client });
        client
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.network.set_time(self.now);
            self.app.update();
            self.now += Self::FRAME;
        }
    }
}

/// A client observes its server's close, as the server's endpoint goes, through the regular
/// Link -> Connection lifecycle and becomes Disconnected with the endpoint's code.
#[test]
fn a_peer_disconnect_disconnects_the_client() {
    let mut harness = Harness::new();
    let client = harness.connect_client();
    harness.frames(3);

    let world = harness.app.world();
    assert!(world.get::<Linked>(client).is_some());
    assert!(world.get::<Connected>(client).is_some());
    assert!(world.get::<Disconnected>(client).is_none());

    harness.app.world_mut().despawn(harness.server);
    harness.frames(2);

    let world = harness.app.world();
    assert!(world.get::<Linked>(client).is_none());
    assert!(world.get::<Connected>(client).is_none());
    assert_eq!(
        world
            .get::<Disconnected>(client)
            .map(|disconnected| &disconnected.reason),
        Some(&DisconnectedReason::LinkFailed(UnlinkReason::ClosedByPeer(
            AppCode::DROPPED
        )))
    );
}

/// The server's endpoint spawns a link for each client at its first packet. Each reaches
/// `Linked + Connected + ClientOf`, so downstream `On<Insert<(Transport, ClientOf)>>` channel
/// observers fire, and the two clients have distinct `PeerId`s, their own addresses.
#[test]
fn each_server_link_reaches_connected_with_a_peer_of_its_own() {
    let mut harness = Harness::new();
    harness.connect_client();
    harness.connect_client();
    harness.frames(3);

    let world = harness.app.world_mut();
    let mut links = world.query_filtered::<Entity, With<LinkOf>>();
    let links: Vec<_> = links.iter(world).collect();
    let [first, second] = links[..] else {
        panic!("the server holds two links: {links:?}");
    };
    let peers = [first, second].map(|link| {
        assert!(world.get::<Linked>(link).is_some(), "Linked");
        assert!(world.get::<Connected>(link).is_some(), "Connected");
        assert!(world.get::<ClientOf>(link).is_some(), "ClientOf");
        let peer = world.get::<RemoteAddress>(link).unwrap().0;
        assert_eq!(
            world.get::<RemoteId>(link),
            Some(&RemoteId(PeerId::Address(peer)))
        );
        peer
    });
    assert_ne!(peers[0], peers[1]);
}
