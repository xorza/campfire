use super::*;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_time::TimeUpdateStrategy;
use campfire_net_connection::RemoteId;
use campfire_net_connection::{
    ClientOf, Connect, Connected, Connecting, Disconnected, DisconnectedReason, HostClient,
    RawClient, RawServer, Start, Started,
};
use campfire_net_core::PeerId;
use campfire_net_emulator::{DuplexModel, EmulatedNetwork, WireClock};
use campfire_net_endpoint::{Endpoint, EndpointPlugin};
use campfire_net_link::{Link, LinkOf, Linked, PeerAddr};
use campfire_net_packets::{EmulatedWires, EndpointConfig, EndpointParts};
use campfire_net_payload::{AppCode, UnlinkReason};
use campfire_net_timeline::{Clock, ClockSource};
use core::time::Duration;

use crate::client_kind::ClientKind;
use crate::stepper::Stepper;
use crate::stepper_config::{Init, StepperConfig};

/// A raw server only becomes `Started` after its IO becomes `Linked`. If the host-client requests
/// a connection first, that request should remain pending and be retried when the server starts.
#[test]
fn a_raw_host_connects_once_its_server_is_linked() {
    let mut config = StepperConfig::with_clients(vec![ClientKind::Host]);
    config.init = Init::Deferred;
    let mut stepper = Stepper::new(ConnectionStack, config);
    let server = stepper.server_entity();
    let host_client = stepper.host_client_entity();
    // The server's IO completes its `LinkStart` only when the test does.
    stepper.server_mut().remove::<Endpoint<EmulatedWires>>();
    stepper.server_app_mut().finish();
    stepper.server_app_mut().cleanup();

    let world = stepper.server_app_mut().world_mut();
    world.trigger(Start { entity: server });
    world.flush();
    world.trigger(Connect {
        entity: host_client,
    });
    world.flush();
    assert!(!stepper.server().contains::<Started>());
    assert!(stepper.host_client().contains::<Connecting>());

    // An asynchronous raw server IO completes its `LinkStart` request.
    stepper.server_mut().insert(Linked);
    stepper.server_app_mut().world_mut().flush();

    assert!(stepper.server().contains::<Started>());
    assert!(stepper.host_client().contains::<Connected>());
    assert!(stepper.host_client().contains::<HostClient>());
    assert!(!stepper.host_client().contains::<Connecting>());
}

/// One app with a raw server's plain endpoint and its raw clients', stepped by hand on the
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
            EndpointPlugin::<EmulatedWires>::default(),
            RawServerPlugin,
            RawClientPlugin,
        ));
        let server = app
            .world_mut()
            .spawn((
                RawServer,
                Endpoint::from(EndpointParts::plain_server(EndpointConfig::default())),
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

    /// A raw client on a wire of its own to the server, which connects.
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
                RawClient,
                Link::default(),
                Endpoint::from(EndpointParts::plain_client(
                    pair.client,
                    EndpointConfig::default(),
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

/// A raw client observes its server's close, as the server's endpoint goes, through the regular
/// Link -> Connection lifecycle and becomes Disconnected with the endpoint's code.
#[test]
fn peer_disconnect_disconnects_raw_client() {
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
fn server_links_reach_connected_with_distinct_peers() {
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
        let peer = world.get::<PeerAddr>(link).unwrap().0;
        assert_eq!(
            world.get::<RemoteId>(link),
            Some(&RemoteId(PeerId::Raw(peer)))
        );
        peer
    });
    assert_ne!(peers[0], peers[1]);
}
