use std::num::NonZeroU32;
use std::sync::Arc;

use bevy_app::{App, First, PostUpdate, TaskPoolPlugin, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules, SingleThreadedExecutor};
use bevy_ecs::system::Commands;
use bevy_state::app::StatesPlugin;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use campfire_capabilities::{Action, Body, MoveStep, Order, Owner, Team};
use campfire_log::internals::LogCheck;
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey, XOnlyPublicKey};
use campfire_protocol::{CertificateHash, SeedChain};
use campfire_runner::InputRules;
use campfire_sim::{EntityIndex, SimTick, StableId, TickRate};
use lightyear::crossbeam::CrossbeamIo;
use lightyear::prelude::client::{ClientPlugins, InputDelayConfig, InputTimelineConfig, RawClient};
use lightyear::prelude::server::{RawServer, ServerPlugins};
use lightyear::prelude::{
    Client, Connect, Connected, Link, LinkOf, LinkSystems, Linked, LocalTimelineSync, PeerAddr,
    PredictionManager, PredictionMetrics, ReplicationReceiver, ReplicationSender, RollbackMode,
    SyncConfig, SyncSystems,
};
use lightyear::transport::plugin::TransportSystems;

use crate::lobby::{Lobby, LobbySetup};
use crate::local_match::delay_line::DelayLine;
use crate::local_match::link_model::LinkModel;
use crate::match_clock::MatchClock;
use crate::net_protocol::NetProtocol;
use crate::order_script::OrderScript;
use crate::sim_client::bot_script::BotScript;
use crate::sim_client::join_state::JoinState;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_client::{PendingOrders, SimClient};
use crate::sim_server::{PlayerLink, SimServer, TickHashes};

pub(crate) mod delay_line;
pub(crate) mod link_model;

/// Frames a connection gets to link and sync its timeline, and a join to start the match.
const CONNECT_FRAMES: usize = 300;
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

/// An app of a match: the server's, or a client's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Server,
    Client(usize),
}

impl MatchSetup {
    /// One player through a perfect link, whose client rolls back only on a misprediction, with
    /// a server that runs a frame a tick.
    pub const SOLO: MatchSetup = MatchSetup::solo(RollbackMode::Check, 1, LocalMatch::SEED_CHAIN);

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

    /// Two players through `link`, whose clients roll back only on a misprediction, with a
    /// server that runs 3 frames a tick.
    pub const fn duo(link: LinkModel, seed_chain: SeedChain) -> MatchSetup {
        MatchSetup {
            players: 2,
            rollback: RollbackMode::Check,
            server_frames: 3,
            link,
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
    packages: Arc<ModePackages>,
    /// Last, so it drops after the apps and sees what they log as they drop.
    log: LogCheck,
}

impl LocalMatch {
    /// The seed chain the tests' servers commit to.
    pub const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);

    /// The match scenario's orders by team, the west then the east: each avatar walks 4 m toward
    /// the enemy tower, which kills it there; after it respawns, it walks to a point near the
    /// middle. From their spawns 2 m apart, the two walk on lines that part, so they never touch.
    /// The first order waits for the clients' lead on the server to settle: Lightyear brings it to
    /// its target by 5 % of a tick a frame.
    pub const SCENARIO_SCRIPTS: [&str; 2] = [
        "[[order]]\ntick = 60\nmove = [4, -2]\n[[order]]\ntick = 450\nmove = [-3, -2]\n",
        "[[order]]\ntick = 60\nmove = [-4, 3]\n[[order]]\ntick = 450\nmove = [3, 4]\n",
    ];

    /// A match of the test lane mode at its default rate, its clients connected and synced.
    pub fn new(setup: MatchSetup) -> LocalMatch {
        let log = LogCheck::start();
        assert!(
            (1..=2).contains(&setup.players),
            "the lane mode takes 1 or 2 players"
        );
        assert!(
            setup.server_frames > 0,
            "the server runs a frame a tick at least"
        );
        let packages = Arc::new(lane_mode());
        let tick_hz = packages.manifest().tick_hz.default();
        let tick = TickRate::new(tick_hz).length();

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
                    DelayLine::new(setup.link, setup.server_frames, stream, tick),
                ))
                .id();
            links.push(link);

            let ClientApp { app, entity } =
                ClientApp::new(&setup, player, &packages, tick_hz, client_io, stream + 1);
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
            log,
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

    /// Opens the session for every client, and steps until each joined, every end runs the
    /// match, and each client holds its player's avatar; see `Lobby`. The players take slots in the
    /// order their joins arrive. A client runs match ticks from the match start's message, and
    /// its avatar comes in the replication after the server's first tick, in another packet: which
    /// arrives first varies with how Lightyear packs and resends them, by the wall clock.
    pub fn start_match(&mut self) {
        let packages = lane_mode();
        let lobby = Lobby::new(LobbySetup {
            tick_hz: packages.manifest().tick_hz.default(),
            packages,
            server_key: server_key(),
            seed_chain: self.setup.seed_chain,
            inputs: InputRules::LAN,
            certificate: CERTIFICATE,
            players: self.setup.players,
            clock: || NOW,
            entropy: |bytes| bytes.fill(5),
        })
        .expect("the lane mode runs at its default rate");
        self.server.world_mut().insert_resource(lobby);
        for _ in 0..CONNECT_FRAMES {
            let playing = |client: &App| client.world().resource::<JoinState>().clock().is_some();
            if self.server.world().contains_resource::<MatchClock>()
                && self.clients.iter().all(playing)
                && (0..self.clients.len()).all(|client| self.holds_hero(client))
            {
                return;
            }
            self.step();
        }
        panic!("the match did not start in {CONNECT_FRAMES} frames");
    }

    /// Whether `client` holds its player's avatar.
    fn holds_hero(&self, client: usize) -> bool {
        let world = self.clients[client].world();
        world
            .resource::<EntityIndex>()
            .get(self.avatar(client))
            .is_some()
    }

    /// One frame of the server alone, which shifts where in a step its ticks fall.
    pub fn server_frame(&mut self) {
        self.server.update();
    }

    /// One frame of each client, then the server's frames: one tick each.
    pub fn step(&mut self) {
        for client in 0..self.clients.len() {
            self.client_frame(client);
        }
        for _ in 0..self.setup.server_frames {
            self.server_frame();
        }
    }

    /// `ticks` frames of each client alone, then one frame of the server as long as their ticks:
    /// a server whose process stalled, and whose clients ran on and sent their inputs meanwhile.
    pub fn stall_server(&mut self, ticks: u32) {
        for _ in 0..ticks {
            for client in 0..self.clients.len() {
                self.client_frame(client);
            }
        }
        let tick = TickRate::new(self.packages.manifest().tick_hz.default()).length();
        let frame = tick / self.setup.server_frames;
        let stalled = TimeUpdateStrategy::ManualDuration(tick * ticks);
        self.server.insert_resource(stalled);
        self.server_frame();
        self.server
            .insert_resource(TimeUpdateStrategy::ManualDuration(frame));
    }

    /// One frame of `client` alone: one tick of it.
    pub fn client_frame(&mut self, client: usize) {
        self.clients[client].update();
    }

    pub const fn setup(&self) -> &MatchSetup {
        &self.setup
    }

    /// Gives `client`'s avatar an order, sent in the client's next tick.
    pub fn order(&mut self, client: usize, action: Action) {
        let unit = self.avatar(client);
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

    /// `client`'s player's avatar, as the server holds it.
    pub fn avatar(&self, client: usize) -> StableId {
        let world = self.server.world();
        let slot = world
            .get::<PlayerLink>(self.links[client])
            .expect("the match started, with the client's slot")
            .slot();
        let mut avatars = world.resource::<EntityIndex>().iter();
        avatars
            .find(|&(_, unit)| {
                world
                    .get::<Owner>(unit)
                    .is_some_and(|owner| owner.slot() == slot)
            })
            .map(|(id, _)| id)
            .expect("the match started, with the player's avatar")
    }

    /// Makes each client play the script of its avatar's team, by team index, as a bot does; gives
    /// each client's team index. Players take slots in the order their joins arrive, so a
    /// scenario cannot fix which client plays which team.
    pub fn play_by_team(&mut self, scripts: [&str; 2]) -> [usize; 2] {
        let teams = [0, 1].map(|client| usize::from(self.team(client).index()));
        assert_ne!(
            teams[0], teams[1],
            "the two players' avatars are on two teams"
        );
        for (client, team) in teams.into_iter().enumerate() {
            let script = OrderScript::parse(scripts[team]).expect("a scenario's script reads");
            self.play(client, script);
        }
        teams
    }

    /// The team of `client`'s player's avatar: players take slots in the order their joins arrive.
    pub fn team(&self, client: usize) -> Team {
        let world = self.server.world();
        let avatar = world.resource::<EntityIndex>().get(self.avatar(client));
        *world
            .get::<Team>(avatar.expect("the avatar exists"))
            .expect("an avatar has a team")
    }

    pub const fn server(&self) -> &App {
        &self.server
    }

    pub fn app(&self, end: End) -> &App {
        match end {
            End::Server => &self.server,
            End::Client(client) => &self.clients[client],
        }
    }

    /// The sim tick `end` runs next.
    pub fn next_tick(&self, end: End) -> u64 {
        self.app(end).world().resource::<SimTick>().start().get()
    }

    /// The times `client` rolled its state back.
    pub fn rollbacks(&self, client: usize) -> u32 {
        let world = self.clients[client].world();
        world.resource::<PredictionMetrics>().rollbacks
    }

    /// The client whose player's avatar is on `team`.
    pub fn client_of(&self, team: Team) -> usize {
        (0..self.clients.len())
            .find(|&client| self.team(client) == team)
            .expect("a client plays each team")
    }

    /// The tower of `team` on the server: its one unit that stands, has a body and no owner.
    pub fn tower(&self, team: Team) -> StableId {
        let world = self.server.world();
        let mut towers = world
            .resource::<EntityIndex>()
            .iter()
            .filter(|&(_, entity)| {
                let unit = world.entity(entity);
                unit.get::<Team>() == Some(&team)
                    && unit.contains::<Body>()
                    && !unit.contains::<MoveStep>()
                    && !unit.contains::<Owner>()
            });
        let (tower, _) = towers.next().expect("a team has a tower");
        assert!(towers.next().is_none(), "a team has one tower");
        tower
    }

    pub const fn server_mut(&mut self) -> &mut App {
        &mut self.server
    }

    pub fn client(&self, client: usize) -> &App {
        &self.clients[client]
    }

    pub fn client_mut(&mut self, client: usize) -> &mut App {
        &mut self.clients[client]
    }

    /// The server's link to `client`.
    pub fn link(&self, client: usize) -> Entity {
        self.links[client]
    }

    /// The packages of the session's mode.
    /// The events at Warn and Error that the match logged and no test took yet.
    pub const fn log(&self) -> &LogCheck {
        &self.log
    }

    pub fn packages(&self) -> &ModePackages {
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
        packages: &Arc<ModePackages>,
        tick_hz: NonZeroU32,
        io: CrossbeamIo,
        stream: u64,
    ) -> ClientApp {
        let secret = u8::try_from(2 * player + 1).expect("a small player");
        let sim_client = SimClient {
            main_key: keypair(secret),
            session_key: keypair(secret + 1),
            server: ServerPin {
                key: server_key(),
                certificate: CERTIFICATE,
                tick_hz,
            },
            packages: Arc::clone(packages),
            clock: || NOW,
            entropy: |bytes| bytes.fill(4),
        };
        let tick = TickRate::new(tick_hz).length();
        let mut client = App::new();
        client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        client.add_plugins(ClientPlugins {
            tick_duration: tick,
        });
        client.add_plugins((NetProtocol, sim_client));
        client.insert_resource(InputTimelineConfig::new(
            SyncConfig::default(),
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
                DelayLine::new(setup.link, 1, stream, tick),
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
        (
            DelayLine::pass
                .after(TransportSystems::Send)
                .before(LinkSystems::Send),
            DelayLine::pin_round_trip.before(SyncSystems::Sync),
        ),
    );
    app.add_systems(First, DelayLine::pin_round_trip);
    app.add_systems(Update, DelayLine::pin_round_trip);
}

pub(crate) fn keypair(secret: u8) -> Keypair {
    let secret = SecretKey::from_byte_array(&[secret; 32]).expect("a valid secret key");
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

/// The server's key, the x of a point on the curve.
pub(crate) fn server_key() -> XOnlyPublicKey {
    XOnlyPublicKey::from_byte_array(&[8; 32]).expect("[8; 32] is the x of a point")
}

fn lane_mode() -> ModePackages {
    ModePackages::from_dir(&PackageDir::workspace("test/modes/lane")).expect("the test mode loads")
}
