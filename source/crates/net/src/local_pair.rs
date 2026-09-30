use std::time::Duration;

use bevy_app::{App, TaskPoolPlugin};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::system::Commands;
use bevy_state::app::StatesPlugin;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use campfire_kit_moba::Order;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{
    Delegation, DelegationTerms, SeedError, ServerSeed, SessionHeader, SessionId, SessionLog,
    SessionPlayer,
};
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
const SESSION_ID: SessionId = SessionId::new([7; 32]);
const SERVER_KEY: [u8; 32] = [8; 32];
const MAIN_SECRET: [u8; 32] = [1; 32];
const SESSION_SECRET: [u8; 32] = [2; 32];

/// A server app and one client app joined by in-process channels, each on a manual clock that
/// advances one tick per `step`, so every run is the same. The client's player holds fixed keys.
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

    /// A connected and synced pair. The client's state rollbacks follow `rollback`.
    pub fn new(rollback: RollbackMode) -> LocalPair {
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

        let sim_client = SimClient {
            session_key: keypair(SESSION_SECRET),
            session_id: SESSION_ID,
            chain_root: delegation().chain_root(),
        };
        let mut client = App::new();
        client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        client.add_plugins(ClientPlugins {
            tick_duration: LocalPair::TICK,
        });
        client.add_plugins((NetProtocol, sim_client));
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

    /// Starts a match with the client as its only player, inputs held up to 10 ticks late and 30
    /// ahead, and the randomness of `server_seed`; see `SimServer::start_match`.
    pub fn start_match(&mut self, server_seed: ServerSeed) -> Result<(), SeedError> {
        let header = SessionHeader {
            session_id: SESSION_ID,
            server_key: SERVER_KEY,
            max_input_delay: 10,
            max_input_lead: 30,
            max_payload_len: 64,
            max_inputs_per_tick: 4,
            seed_commitment: server_seed.commitment(),
            players: vec![SessionPlayer {
                delegation: delegation(),
                seed_contribution: [4; 32],
            }],
        };
        let log = SessionLog::new(header).expect("the delegation names this session");
        SimServer::start_match(self.server.world_mut(), log, server_seed, &[self.link])
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

fn keypair(secret: [u8; 32]) -> Keypair {
    let secret = SecretKey::from_byte_array(&secret).expect("a valid secret key");
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

/// The player's main key lets their session key sign in the pair's session.
fn delegation() -> Delegation {
    let terms = DelegationTerms {
        session_key: keypair(SESSION_SECRET).x_only_public_key().0,
        server_key: SERVER_KEY,
        session_id: SESSION_ID,
        expiration: 1_700_086_400,
    };
    Delegation::sign(
        &Secp256k1::new(),
        &keypair(MAIN_SECRET),
        &terms,
        1_700_000_000,
        &[0; 32],
    )
}
