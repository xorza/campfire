use std::path::Path;
use std::time::Duration;

use bevy_app::{App, PostUpdate, TaskPoolPlugin};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules, SingleThreadedExecutor};
use bevy_ecs::system::Commands;
use bevy_state::app::StatesPlugin;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use campfire_capabilities::{Action, Order, Owner};
use campfire_package::ModePackages;
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey};
use campfire_protocol::{CertificateHash, SeedChain};
use campfire_sim::{EntityIndex, StableId, TickRate};
use lightyear::crossbeam::CrossbeamIo;
use lightyear::prelude::client::{ClientPlugins, InputDelayConfig, InputTimelineConfig, RawClient};
use lightyear::prelude::server::{RawServer, ServerPlugins};
use lightyear::prelude::{
    Client, Connect, Connected, Link, LinkOf, LinkSystems, Linked, LocalTimelineSync, PeerAddr,
    PredictionManager, ReplicationReceiver, ReplicationSender, RollbackMode, SyncConfig,
};
use lightyear::transport::plugin::TransportSystems;

use crate::lobby::{Lobby, LobbySetup};
use crate::local_match::delay_line::DelayLine;
use crate::local_match::link_model::LinkModel;
use crate::match_clock::MatchClock;
use crate::net_protocol::NetProtocol;
use crate::order_script::OrderScript;
use crate::sim_client::bot_script::BotScript;
use crate::sim_client::client_mode::ClientMode;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_client::{PendingOrders, SimClient};
use crate::sim_server::{PlayerLink, SimServer, TickHashes};

pub(crate) mod delay_line;
pub(crate) mod link_model;

/// The test mode: a lane with a tower a side and a hero for each; player 0 plays the walker, and
/// player 1 the runner.
const LANE_MODE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/test/modes/lane"
);
/// Frames a connection gets to link and sync its timeline, and a join to start the match.
const CONNECT_FRAMES: usize = 300;
const SERVER_KEY: [u8; 32] = [8; 32];
/// In-process channels have no TLS; both ends take this as the certificate's hash.
const CERTIFICATE: CertificateHash = CertificateHash::new([3; 32]);
/// Unix seconds, on every end.
const NOW: u64 = 1_700_000_000;

/// What a `LocalMatch` runs: how many players join, and how.
#[derive(Debug, Clone, Copy)]
pub struct MatchSetup {
    /// 1 or 2: the lane mode has a slot a side.
    pub players: usize,
    /// When the clients roll their state back.
    pub rollback: RollbackMode,
    /// Server frames a tick: a real server runs frames faster than ticks.
    pub server_frames: u32,
    /// How each link carries packets, both ways.
    pub link: LinkModel,
    /// The seed chain the server commits to.
    pub seed_chain: SeedChain,
}

impl MatchSetup {
    /// One player, through perfect links.
    pub const fn solo(
        rollback: RollbackMode,
        server_frames: u32,
        seed_chain: SeedChain,
    ) -> MatchSetup {
        MatchSetup {
            players: 1,
            rollback,
            server_frames,
            link: LinkModel::PERFECT,
            seed_chain,
        }
    }
}

/// A server app and one client app for each player, joined by in-process channels through a
/// `LinkModel`, each on a manual clock that advances one tick per `step`, so every run repeats
/// exactly. Each player holds fixed keys and draws fixed random bytes, and the server keeps the
/// state hash after every tick.
#[derive(Debug)]
pub struct LocalMatch {
    server: App,
    clients: Vec<App>,
    /// The server's link to each client.
    links: Vec<Entity>,
    setup: MatchSetup,
    packages: ModePackages,
}

impl LocalMatch {
    /// A match of the test lane mode at its default rate, its clients connected and synced.
    pub fn new(setup: MatchSetup) -> LocalMatch {
        assert!(
            (1..=2).contains(&setup.players),
            "the lane mode takes 1 or 2 players"
        );
        assert!(
            setup.server_frames > 0,
            "the server runs a frame a tick at least"
        );
        let packages = lane_mode();
        let mode = ClientMode::of(&packages);
        let tick = TickRate::new(mode.tick_hz).length();

        let mut server = App::new();
        server.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        server.add_plugins(ServerPlugins {
            tick_duration: tick,
        });
        server.add_plugins((NetProtocol, SimServer));
        server.init_resource::<TickHashes>();
        let frame = tick / setup.server_frames;
        assert_eq!(
            frame * setup.server_frames,
            tick,
            "the frames make a whole tick"
        );
        server.insert_resource(TimeUpdateStrategy::ManualDuration(frame));
        server.add_observer(
            |added: On<'_, '_, Add, LinkOf>, mut commands: Commands<'_, '_>| {
                commands.entity(added.entity).insert(ReplicationSender);
            },
        );
        pass_through_delay_lines(&mut server);
        // A raw server starts once linked, and in-process channels have no socket to link it.
        let server_entity = server.world_mut().spawn((RawServer, Linked)).id();

        let mut clients = Vec::with_capacity(setup.players);
        let mut links = Vec::with_capacity(setup.players);
        let mut client_entities = Vec::with_capacity(setup.players);
        for player in 0..setup.players {
            let (client_io, server_io) = CrossbeamIo::new_pair();
            let stream = 2 * player as u64;
            let link = server
                .world_mut()
                .spawn((
                    LinkOf {
                        server: server_entity,
                    },
                    Link::default(),
                    PeerAddr(
                        format!("127.0.0.1:{}", player + 1)
                            .parse()
                            .expect("an address"),
                    ),
                    Linked,
                    server_io,
                    DelayLine::new(setup.link, setup.server_frames, stream),
                ))
                .id();
            links.push(link);

            let ClientApp { app, entity } =
                ClientApp::new(&setup, player, &mode, tick, client_io, stream + 1);
            client_entities.push(entity);
            clients.push(app);
        }

        server.finish();
        server.cleanup();
        run_in_order(&mut server);
        for (client, &entity) in clients.iter_mut().zip(&client_entities) {
            client.finish();
            client.cleanup();
            run_in_order(client);
            client.world_mut().trigger(Connect { entity });
        }
        let mut local = LocalMatch {
            server,
            clients,
            links,
            setup,
            packages,
        };
        for _ in 0..CONNECT_FRAMES {
            let synced = local
                .clients
                .iter()
                .zip(&client_entities)
                .all(|(client, &entity)| {
                    let world = client.world();
                    world.entity(entity).contains::<Connected>()
                        && world
                            .get_resource::<LocalTimelineSync>()
                            .is_some_and(LocalTimelineSync::is_synced)
                });
            if synced {
                return local;
            }
            local.step();
        }
        panic!("the clients did not connect and sync in {CONNECT_FRAMES} frames");
    }

    /// Opens the session for every client, and steps until each joined and every end runs the
    /// match; see `Lobby`. The players take slots in the order their joins arrive.
    pub fn start_match(&mut self) {
        let lobby = Lobby::new(LobbySetup {
            packages: lane_mode(),
            server_key: SERVER_KEY,
            seed_chain: self.setup.seed_chain,
            certificate: CERTIFICATE,
            players: self.setup.players,
            clock: || NOW,
            entropy: |bytes| bytes.fill(5),
        });
        self.server.world_mut().insert_resource(lobby);
        for _ in 0..CONNECT_FRAMES {
            let started = |app: &App| app.world().contains_resource::<MatchClock>();
            if started(&self.server) && self.clients.iter().all(started) {
                return;
            }
            self.step();
        }
        panic!("the match did not start in {CONNECT_FRAMES} frames");
    }

    /// One frame of the server alone, which shifts where in a step its ticks fall.
    pub fn server_frame(&mut self) {
        self.server.update();
    }

    /// One frame of each client, then the server's frames: one tick each.
    pub fn step(&mut self) {
        for client in &mut self.clients {
            client.update();
        }
        for _ in 0..self.setup.server_frames {
            self.server.update();
        }
    }

    /// Gives `client`'s hero an order, sent in the client's next tick.
    pub fn order(&mut self, client: usize, action: Action) {
        let unit = self.hero(client);
        self.clients[client]
            .world_mut()
            .resource_mut::<PendingOrders>()
            .push(Order { unit, action });
    }

    /// Makes `client` play `script`, as a bot does.
    pub fn play(&mut self, client: usize, script: OrderScript) {
        self.clients[client]
            .world_mut()
            .insert_resource(BotScript::new(script));
    }

    /// `client`'s player's hero, as the server holds it.
    pub fn hero(&self, client: usize) -> StableId {
        let world = self.server.world();
        let slot = world
            .get::<PlayerLink>(self.links[client])
            .expect("the match started, with the client's slot")
            .slot();
        let mut heroes = world.resource::<EntityIndex>().iter();
        heroes
            .find(|&(_, unit)| {
                world
                    .get::<Owner>(unit)
                    .is_some_and(|owner| owner.slot() == slot)
            })
            .map(|(id, _)| id)
            .expect("the match started, with the player's hero")
    }

    pub const fn server(&self) -> &App {
        &self.server
    }

    pub const fn server_mut(&mut self) -> &mut App {
        &mut self.server
    }

    pub fn client(&self, client: usize) -> &App {
        &self.clients[client]
    }

    /// The server's link to `client`.
    pub fn link(&self, client: usize) -> Entity {
        self.links[client]
    }

    /// The packages of the session's mode.
    pub const fn packages(&self) -> &ModePackages {
        &self.packages
    }
}

/// A client app of a `LocalMatch`, and its client entity.
#[derive(Debug)]
struct ClientApp {
    app: App,
    entity: Entity,
}

impl ClientApp {
    /// The app of `player`, whose link to the server is `io`, through a delay line of `stream`.
    fn new(
        setup: &MatchSetup,
        player: usize,
        mode: &ClientMode,
        tick: Duration,
        io: CrossbeamIo,
        stream: u64,
    ) -> ClientApp {
        // Keys of 1 and 2 for the first player, 3 and 4 for the second.
        let secret = u8::try_from(2 * player + 1).expect("a small player");
        let sim_client = SimClient {
            main_key: keypair(secret),
            session_key: keypair(secret + 1),
            server: ServerPin {
                key: SERVER_KEY,
                certificate: CERTIFICATE,
            },
            mode: mode.clone(),
            clock: || NOW,
            entropy: |bytes| bytes.fill(4),
        };
        let mut client = App::new();
        client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        client.add_plugins(ClientPlugins {
            tick_duration: tick,
        });
        client.add_plugins((NetProtocol, sim_client));
        // Lightyear measures the round trip by the wall clock, and a step of this match takes
        // almost none, so the sync margin carries the one-way delay the link model adds, in
        // ticks: its delay and jitter, a tick to spare, and the drift the sync allows before
        // it corrects the lead.
        let sync = SyncConfig::default();
        let spare = u16::try_from(setup.link.delay + setup.link.jitter + 1)
            .expect("a delay of a few steps");
        let margin = f32::from(spare) + sync.error_margin;
        client.insert_resource(InputTimelineConfig::new(
            SyncConfig {
                jitter_margin: margin,
                ..sync
            },
            InputDelayConfig::no_input_delay(),
        ));
        client.insert_resource(TimeUpdateStrategy::ManualDuration(tick));
        let mut prediction = PredictionManager::default();
        prediction.rollback_policy.state = setup.rollback;
        client.insert_resource(prediction);
        pass_through_delay_lines(&mut client);
        let entity = client
            .world_mut()
            .spawn((
                Client,
                RawClient,
                ReplicationReceiver,
                io,
                DelayLine::new(setup.link, 1, stream),
            ))
            .id();
        ClientApp {
            app: client,
            entity,
        }
    }
}

/// Runs every schedule of `app` on one thread, so systems with no order between them keep the same
/// order in every run: Lightyear's schedules have such systems, and the parallel executor, which
/// Bevy's `multi_threaded` feature turns on wherever a workspace build enables it, orders them
/// by timing.
fn run_in_order(app: &mut App) {
    let mut schedules = app.world_mut().resource_mut::<Schedules>();
    for (_, schedule) in schedules.iter_mut() {
        schedule.set_executor(SingleThreadedExecutor::new());
    }
}

/// Runs each link's sends through its delay line, after the transport queued them and before the
/// link flushes them.
fn pass_through_delay_lines(app: &mut App) {
    app.add_systems(
        PostUpdate,
        DelayLine::pass
            .after(TransportSystems::Send)
            .before(LinkSystems::Send),
    );
}

fn keypair(secret: u8) -> Keypair {
    let secret = SecretKey::from_byte_array(&[secret; 32]).expect("a valid secret key");
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

fn lane_mode() -> ModePackages {
    ModePackages::from_dir(Path::new(LANE_MODE)).expect("the test mode loads")
}
