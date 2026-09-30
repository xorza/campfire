use std::path::Path;

use bevy_app::{App, TaskPoolPlugin};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::system::Commands;
use bevy_state::app::StatesPlugin;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use campfire_capabilities::{Action, Order, Owner};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{
    Delegation, DelegationTerms, SeedChain, SessionHeader, SessionLog, SessionTerms,
};
use campfire_runner::{ModePackages, RELEASE, StartError};
use campfire_sim::{EntityIndex, PlayerSlot, StableId};
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

/// The test mode: a lane with a tower a side, and the one hero, which the client's player plays.
const LANE_MODE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/test/modes/lane"
);
/// Frames a connection gets to link and sync its timeline.
const CONNECT_FRAMES: usize = 200;
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
    terms: SessionTerms,
    seed_chain: SeedChain,
    packages: ModePackages,
}

impl LocalPair {
    /// A connected and synced pair in a session of the test lane mode whose server commits to
    /// `seed_chain`, at the mode's default rate, with inputs held up to 10 ticks late and 30
    /// ahead. The client's state rollbacks follow `rollback`.
    pub fn new(rollback: RollbackMode, seed_chain: SeedChain) -> LocalPair {
        let packages = ModePackages::from_dir(Path::new(LANE_MODE)).expect("the test mode loads");
        let terms = SessionTerms {
            server_key: SERVER_KEY,
            tick_hz: packages.manifest().tick_hz.default(),
            max_input_delay: 10,
            max_input_lead: 30,
            max_payload_len: 64,
            max_inputs_per_tick: 4,
            seed_commitment: seed_chain.commitment(),
            release: RELEASE.to_owned(),
            mode: packages.fingerprint(),
            dependencies: packages.dependencies().collect(),
        };
        let tick = terms.tick_length();
        let (client_io, server_io) = CrossbeamIo::new_pair();

        let mut server = App::new();
        server.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        server.add_plugins(ServerPlugins {
            tick_duration: tick,
        });
        server.add_plugins((NetProtocol, SimServer));
        server.insert_resource(TimeUpdateStrategy::ManualDuration(tick));
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
            terms: terms.clone(),
            chain_root: delegation(&terms).chain_root(),
            capabilities: packages.manifest().capabilities,
        };
        let mut client = App::new();
        client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        client.add_plugins(ClientPlugins {
            tick_duration: tick,
        });
        client.add_plugins((NetProtocol, sim_client));
        client.insert_resource(TimeUpdateStrategy::ManualDuration(tick));
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
            terms,
            seed_chain,
            packages,
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

    /// Starts the session's match with the client as its only player; see
    /// `SimServer::start_match`.
    pub fn start_match(&mut self) -> Result<(), StartError> {
        let header = SessionHeader {
            terms: self.terms.clone(),
            players: vec![delegation(&self.terms)],
        };
        let log = SessionLog::new(header).expect("the delegation names this session");
        let server_seed = self.seed_chain.seed(0);
        let world = self.server.world_mut();
        SimServer::start_match(world, log, server_seed, &self.packages, &[self.link])
    }

    /// One frame of each app, the client first: one tick each.
    pub fn step(&mut self) {
        self.client.update();
        self.server.update();
    }

    /// Gives the client's hero an order, sent in the client's next tick.
    pub fn order(&mut self, action: Action) {
        let order = Order {
            unit: self.hero(),
            action,
        };
        self.client
            .world_mut()
            .resource_mut::<PendingOrders>()
            .push(order);
    }

    /// The client's hero, as the server's world holds it.
    fn hero(&self) -> StableId {
        let world = self.server.world();
        world
            .resource::<EntityIndex>()
            .iter()
            .find(|&(_, unit)| {
                world
                    .entity(unit)
                    .get::<Owner>()
                    .is_some_and(|owner| owner.slot() == PlayerSlot::new(0))
            })
            .map(|(id, _)| id)
            .expect("the match started, with the client's hero")
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

    /// The packages of the session's mode.
    pub const fn packages(&self) -> &ModePackages {
        &self.packages
    }
}

fn keypair(secret: [u8; 32]) -> Keypair {
    let secret = SecretKey::from_byte_array(&secret).expect("a valid secret key");
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

/// The player's main key lets their session key sign in the session of `session`.
fn delegation(session: &SessionTerms) -> Delegation {
    let terms = DelegationTerms {
        session_key: keypair(SESSION_SECRET).x_only_public_key().0,
        server_key: SERVER_KEY,
        session_id: session.session_id(),
        seed_contribution: [4; 32],
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
