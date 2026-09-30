use std::time::Duration;

use bevy_app::{App, TaskPoolPlugin};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::system::Commands;
use bevy_state::app::StatesPlugin;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use campfire_kit_moba::Order;
use campfire_protocol::{InputHash, SeedError, ServerSeed, SessionHeader};
use lightyear::crossbeam::CrossbeamIo;
use lightyear::prelude::client::{ClientPlugins, RawClient};
use lightyear::prelude::server::{RawServer, ServerPlugins};
use lightyear::prelude::{
    Client, Connect, Connected, Link, LinkOf, Linked, LocalTimelineSync, PeerAddr,
    PredictionManager, ReplicationReceiver, ReplicationSender, RollbackMode,
};

use crate::net_protocol::NetProtocol;
use crate::sim_client::{PendingOrders, SimClient};
use crate::sim_server::SimServer;

/// Frames a connection gets to link and sync its timeline.
const CONNECT_FRAMES: usize = 200;

/// A server app and one client app joined by in-process channels, each on a manual clock that
/// advances one tick per `step`, so every run is the same.
#[derive(Debug)]
pub struct LocalPair {
    server: App,
    client: App,
    /// The server's link to the client.
    link: Entity,
}

impl LocalPair {
    /// 30 ticks a second, the MOBA's default.
    pub const TICK: Duration = Duration::from_nanos(1_000_000_000 / 30);

    /// A connected and synced pair. The client's first input links to `chain_root`, and its
    /// state rollbacks follow `rollback`.
    pub fn new(chain_root: InputHash, rollback: RollbackMode) -> LocalPair {
        let (client_io, server_io) = CrossbeamIo::new_pair();

        let mut server = App::new();
        server.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        server.add_plugins(ServerPlugins {
            tick_duration: LocalPair::TICK,
        });
        server.add_plugins((NetProtocol, SimServer));
        server.insert_resource(TimeUpdateStrategy::ManualDuration(LocalPair::TICK));
        server.add_observer(
            |added: On<'_, '_, Add, LinkOf>, mut commands: Commands<'_, '_>| {
                commands.entity(added.entity).insert(ReplicationSender);
            },
        );
        // A raw server starts once linked, and in-process channels have no socket to link it.
        let server_entity = server.world_mut().spawn((RawServer, Linked)).id();
        let link = server
            .world_mut()
            .spawn((
                LinkOf {
                    server: server_entity,
                },
                Link::default(),
                PeerAddr("127.0.0.1:1".parse().expect("a socket address")),
                Linked,
                server_io,
            ))
            .id();

        let mut client = App::new();
        client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        client.add_plugins(ClientPlugins {
            tick_duration: LocalPair::TICK,
        });
        client.add_plugins((NetProtocol, SimClient { chain_root }));
        client.insert_resource(TimeUpdateStrategy::ManualDuration(LocalPair::TICK));
        let mut prediction = PredictionManager::default();
        prediction.rollback_policy.state = rollback;
        client.insert_resource(prediction);
        let client_entity = client
            .world_mut()
            .spawn((Client, RawClient, ReplicationReceiver, client_io))
            .id();

        for app in [&mut server, &mut client] {
            app.finish();
            app.cleanup();
        }
        client.world_mut().trigger(Connect {
            entity: client_entity,
        });
        let mut pair = LocalPair {
            server,
            client,
            link,
        };
        for _ in 0..CONNECT_FRAMES {
            let world = pair.client.world();
            if world.entity(client_entity).contains::<Connected>()
                && world
                    .get_resource::<LocalTimelineSync>()
                    .is_some_and(LocalTimelineSync::is_synced)
            {
                return pair;
            }
            pair.step();
        }
        panic!("the client did not connect and sync in {CONNECT_FRAMES} frames");
    }

    /// Starts the match of `header` with the client as its only player; see
    /// `SimServer::start_match`.
    pub fn start_match(
        &mut self,
        header: SessionHeader,
        server_seed: ServerSeed,
    ) -> Result<(), SeedError> {
        SimServer::start_match(self.server.world_mut(), header, server_seed, &[self.link])
    }

    /// One frame of each app, the client first: one tick each.
    pub fn step(&mut self) {
        self.client.update();
        self.server.update();
    }

    /// Gives the client's player an order, sent in its next tick.
    pub fn order(&mut self, order: Order) {
        self.client
            .world_mut()
            .resource_mut::<PendingOrders>()
            .push(order);
    }

    pub const fn server(&self) -> &App {
        &self.server
    }

    pub const fn server_mut(&mut self) -> &mut App {
        &mut self.server
    }

    pub const fn client(&self) -> &App {
        &self.client
    }

    pub const fn link(&self) -> Entity {
        self.link
    }
}
