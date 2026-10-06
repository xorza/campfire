use std::cell::Cell;
use std::fmt::Write as _;
use std::mem;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bevy_app::{App, First, PostUpdate, TaskPoolPlugin, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules, SingleThreadedExecutor};
use bevy_ecs::system::Commands;
use bevy_state::app::StatesPlugin;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use campfire_capabilities::{
    Action, Body, Leaver, MoveStep, Order, Owner, PlayersData, SaveBy, Team,
};
use campfire_common::PlayerSlot;
use campfire_log::internals::LogCheck;
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::secp256k1::{Keypair, Secp256k1, SecretKey, XOnlyPublicKey};
use campfire_protocol::{CertificateHash, SeedChain, ServerInput, SessionPrivate};
use campfire_runner::{InputRules, Session};
use campfire_sim::{EntityIndex, SimTick, StableId, TickRate};
use lightyear::crossbeam::CrossbeamIo;
use lightyear::prelude::client::{ClientPlugins, InputDelayConfig, InputTimelineConfig, RawClient};
use lightyear::prelude::server::{RawServer, ServerPlugins};
use lightyear::prelude::{
    Client, Connect, Connected, Link, LinkOf, LinkSystems, Linked, LocalTimelineSync, PeerAddr,
    PredictionManager, PredictionMetrics, ReplicationReceiver, ReplicationSender, RollbackMode,
    SyncConfig, SyncSystems, Unlink, UnlinkReason,
};
use lightyear::transport::plugin::TransportSystems;

use crate::lobby::{Lobby, LobbySetup};
use crate::local_match::delay_line::DelayLine;
use crate::local_match::link_model::LinkModel;
use crate::local_pace::LocalPace;
use crate::local_session::LocalSession;
use crate::match_clock::MatchClock;
use crate::net_protocol::NetProtocol;
use crate::order_script::OrderScript;
use crate::pace::Pace;
use crate::server_bots::{ServerBots, SlotBot};
use crate::server_setup::ServerSetup;
use crate::session_dir::SessionDir;
use crate::session_times::SessionTimes;
use crate::sim_client::bot_script::BotScript;
use crate::sim_client::join_state::JoinState;
use crate::sim_client::receipt_writer::ReceiptWriter;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_client::{PendingOrders, SimClient};
use crate::sim_server::{PlayerLink, SimServer, TickHashes};

pub(crate) mod delay_line;
pub(crate) mod link_model;

/// Frames a connection gets to link and sync its timeline, and a join to start the match.
const CONNECT_FRAMES: usize = 300;
/// In-process channels have no TLS; both ends take this as the certificate's hash.
const CERTIFICATE: CertificateHash = CertificateHash::new([3; 32]);
/// Unix seconds, on every end, until a test sets the clock.
const NOW: u64 = 1_700_000_000;

thread_local! {
    /// The Unix time every end's clock reads: a match runs on the test's thread.
    static UNIX_NOW: Cell<u64> = const { Cell::new(NOW) };
}

fn unix_now() -> u64 {
    UNIX_NOW.with(Cell::get)
}

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
    /// How long the server waits for a player whose link fails, and restores a stopped session.
    pub times: SessionTimes,
    /// The lane mode's `[players]`.
    pub rules: PlayersData,
    /// The lane mode's `[saves] by`.
    pub save_by: SaveBy,
    /// The script of the server's bot in the slot after the players', when it plays one.
    pub bot: Option<&'static str>,
    /// The script of the server's bot in a slot its player left, when it plays one.
    pub takeover: Option<&'static str>,
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
            times: SessionTimes::DEFAULT,
            rules: PlayersData::DEFAULT,
            save_by: SaveBy::Player,
            bot: None,
            takeover: None,
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
            times: SessionTimes::DEFAULT,
            rules: PlayersData::DEFAULT,
            save_by: SaveBy::Player,
            bot: None,
            takeover: None,
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
    /// The server's raw server entity, which its links belong to.
    server_entity: Entity,
    clients: Vec<App>,
    /// Each client app's client entity.
    client_entities: Vec<Entity>,
    /// The server's link to each client.
    links: Vec<Entity>,
    /// The links made so far, which numbers each link's address and delay lines.
    links_made: u64,
    setup: MatchSetup,
    packages: Arc<ModePackages>,
    /// The server's data directory, where it keeps its session, once a test gives it one.
    data: Option<PathBuf>,
    /// The pause and the speed every end follows.
    pace: Arc<Pace>,
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
        let packages = Arc::new(lane_mode(setup.rules, setup.save_by));
        let tick_hz = packages.manifest().tick_hz.default();
        let tick = TickRate::new(tick_hz).length();

        let pace = Arc::new(Pace::default());
        let mut server = LocalMatch::server_app(&setup, tick, &pace);
        // A raw server starts once linked, and in-process channels have no socket to link it.
        let server_entity = server.world_mut().spawn((RawServer, Linked)).id();
        server.finish();
        server.cleanup();
        run_in_order(&mut server);
        let mut local = LocalMatch {
            server,
            server_entity,
            clients: Vec::with_capacity(setup.players),
            client_entities: Vec::with_capacity(setup.players),
            links: Vec::with_capacity(setup.players),
            links_made: 0,
            setup,
            packages,
            data: None,
            pace,
            log,
        };
        for player in 0..setup.players {
            local.add_client(player);
        }
        for _ in 0..CONNECT_FRAMES {
            let synced =
                local
                    .clients
                    .iter()
                    .zip(&local.client_entities)
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

    /// The server's app, its frames `setup.server_frames` a tick of `tick`, before any link.
    fn server_app(setup: &MatchSetup, tick: Duration, pace: &Arc<Pace>) -> App {
        let mut server = App::new();
        server.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        server.add_plugins(ServerPlugins {
            tick_duration: tick,
        });
        server.add_plugins((NetProtocol, SimServer));
        server.insert_resource(LocalSession);
        server.add_plugins(LocalPace {
            pace: Arc::clone(pace),
            tick,
        });
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
        server
    }

    /// Adds a client of `player`, whose keys it holds, linked to the server and connecting; the
    /// index of the new client. A client of a player another client plays logs in again.
    pub fn add_client(&mut self, player: usize) -> usize {
        let tick_hz = self.packages.manifest().tick_hz.default();
        let ClientApp { mut app, entity } =
            ClientApp::new(&self.setup, player, &self.packages, tick_hz, &self.pace);
        app.finish();
        app.cleanup();
        run_in_order(&mut app);
        self.clients.push(app);
        self.client_entities.push(entity);
        self.links.push(Entity::PLACEHOLDER);
        let client = self.clients.len() - 1;
        self.mend_link(client);
        self.clients[client].world_mut().trigger(Connect { entity });
        client
    }

    /// Gives `client` a new link to the server through a new pair of in-process channels and
    /// delay lines, as a network that works again does: a client that tries its link again
    /// connects through it once its wait passed.
    pub fn mend_link(&mut self, client: usize) {
        let (client_io, server_io) = CrossbeamIo::new_pair();
        let stream = 2 * self.links_made;
        self.links_made += 1;
        let tick = TickRate::new(self.packages.manifest().tick_hz.default()).length();
        let address = format!("127.0.0.1:{}", self.links_made)
            .parse()
            .expect("an address");
        self.links[client] = self
            .server
            .world_mut()
            .spawn((
                LinkOf {
                    server: self.server_entity,
                },
                Link::default(),
                PeerAddr(address),
                Linked,
                server_io,
                DelayLine::new(self.setup.link, self.setup.server_frames, stream, tick),
            ))
            .id();
        let entity = self.client_entities[client];
        self.clients[client].world_mut().entity_mut(entity).insert((
            client_io,
            DelayLine::new(self.setup.link, 1, stream + 1, tick),
        ));
    }

    /// Cuts `client`'s link, as a network that fails does: the server and the client each see
    /// it end.
    pub fn cut_link(&mut self, client: usize) {
        let reason = || UnlinkReason::TransportError("the test cut the link".to_owned());
        let link = self.links[client];
        self.server.world_mut().trigger(Unlink {
            entity: link,
            reason: reason(),
        });
        let entity = self.client_entities[client];
        self.clients[client].world_mut().trigger(Unlink {
            entity,
            reason: reason(),
        });
    }

    /// Sets the Unix time every end's clock reads, in seconds.
    pub fn set_clock(&mut self, seconds: u64) {
        UNIX_NOW.with(|now| now.set(seconds));
    }

    /// Every input the server logged, in the order logged.
    pub fn server_inputs(&self) -> Vec<ServerInput> {
        let session = self.server.world().resource::<Session>();
        session.log().server_inputs().cloned().collect()
    }

    /// The server's key pair, which signs what it logs.
    pub fn server_keypair() -> Keypair {
        server_keypair()
    }

    /// What the server opens or restores its session with: its key, the certificate hash both
    /// ends take, the setup's times, a fixed clock and fixed random bytes.
    fn server_setup(&self) -> ServerSetup {
        ServerSetup {
            key: server_keypair(),
            certificate: CERTIFICATE,
            times: self.setup.times,
            clock: unix_now,
            entropy: |bytes| bytes.fill(5),
        }
    }

    /// The bots of the setup: its bot in the slot after the players', and its takeover bot.
    fn server_bots(&self) -> ServerBots {
        let script = |text| OrderScript::parse(text).expect("a test's bot script reads");
        let slots = self.setup.bot.map(|text| SlotBot {
            slot: PlayerSlot::new(u32::try_from(self.setup.players).expect("a small count")),
            script: script(text),
        });
        ServerBots {
            slots: slots.into_iter().collect(),
            takeover: self.setup.takeover.map(script),
        }
    }

    /// Gives the server the data directory `dir`, where the session `start_match` opens keeps its
    /// directory, private record and journal, and each client `client-<index>` in it, where it
    /// writes its receipts.
    pub fn keep_data(&mut self, dir: PathBuf) {
        assert!(
            !self.server.world().contains_resource::<MatchClock>(),
            "a server keeps its data from before the match"
        );
        for (client, app) in self.clients.iter_mut().enumerate() {
            let own = dir.join(format!("client-{client}"));
            app.world_mut().insert_resource(ReceiptWriter::start(own));
        }
        self.data = Some(dir);
    }

    /// Ends the server, as a crash after its journal's last sync does; each client sees its
    /// link end.
    pub fn stop_server(&mut self) {
        drop(mem::replace(&mut self.server, App::new()));
        let reason = || UnlinkReason::TransportError("the server stopped".to_owned());
        for (client, &entity) in self.clients.iter_mut().zip(&self.client_entities) {
            client.world_mut().trigger(Unlink {
                entity,
                reason: reason(),
            });
        }
    }

    /// Ends the server, then starts a new one on its data directory, which restores the session
    /// its journal holds; see `SimServer::restore_match`. Each client gets a new link to it, which
    /// it connects through once its wait passed.
    pub fn restart_server(&mut self) {
        self.stop_server();
        let data = self.data.as_ref().expect("a server with a data directory");
        let dir = SessionDir::find(data)
            .unwrap_or_else(|error| panic!("{error}"))
            .expect("a session the stop ended");
        let session = dir
            .restore()
            .unwrap_or_else(|error| panic!("{error}"))
            .expect("a session whose match started");
        let tick = TickRate::new(self.packages.manifest().tick_hz.default()).length();
        let mut server = LocalMatch::server_app(&self.setup, tick, &self.pace);
        self.server_entity = server.world_mut().spawn((RawServer, Linked)).id();
        server.finish();
        server.cleanup();
        run_in_order(&mut server);
        server.update();
        SimServer::restore_match(
            server.world_mut(),
            session,
            &self.packages,
            &self.server_setup(),
            self.server_bots(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        self.server = server;
        for client in 0..self.clients.len() {
            self.mend_link(client);
        }
    }

    /// Opens the session for every client, and steps until each joined, every end runs the
    /// match, and each client holds its player's avatar; see `Lobby`. The players take slots in the
    /// order their joins arrive. A client runs match ticks from the match start's message, and
    /// its avatar comes in the replication after the server's first tick, in another packet: which
    /// arrives first varies with how Lightyear packs and resends them, by the wall clock.
    pub fn start_match(&mut self) {
        let packages = Arc::clone(&self.packages);
        let mut lobby = Lobby::new(LobbySetup {
            tick_hz: packages.manifest().tick_hz.default(),
            packages,
            seed_chain: self.setup.seed_chain,
            inputs: InputRules::LAN,
            slots: self.setup.players + usize::from(self.setup.bot.is_some()),
            bots: self.server_bots(),
            open: Vec::new(),
            server: self.server_setup(),
        })
        .expect("the lane mode runs at its default rate");
        if let Some(data) = &self.data {
            let private = SessionPrivate {
                seed_chain: self.setup.seed_chain,
                terms: lobby.terms().clone(),
            };
            let dir = SessionDir::create(data, &private).unwrap_or_else(|error| panic!("{error}"));
            let files = dir.start().unwrap_or_else(|error| panic!("{error}"));
            lobby.keep_files(files);
        }
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

    /// One frame of each client, then the server's frames: one tick each. A server whose session
    /// went back to a save then starts again on its data, as a local server does.
    pub fn step(&mut self) {
        for client in 0..self.clients.len() {
            self.client_frame(client);
        }
        for _ in 0..self.setup.server_frames {
            self.server_frame();
        }
        if SimServer::reload_wanted(self.server.world()) {
            self.restart_server();
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

    /// The pause and the speed every end follows.
    pub fn pace(&self) -> &Pace {
        &self.pace
    }

    /// The events at Warn and Error that the match logged and no test took yet.
    pub const fn log(&self) -> &LogCheck {
        &self.log
    }

    /// The packages of the session's mode.
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
    /// The app of `player`, following `pace`.
    fn new(
        setup: &MatchSetup,
        player: usize,
        packages: &Arc<ModePackages>,
        tick_hz: NonZeroU32,
        pace: &Arc<Pace>,
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
            clock: unix_now,
            entropy: |bytes| bytes.fill(4),
            data: None,
        };
        let tick = TickRate::new(tick_hz).length();
        let mut client = App::new();
        client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        client.add_plugins(ClientPlugins {
            tick_duration: tick,
        });
        client.add_plugins((NetProtocol, sim_client));
        client.add_plugins(LocalPace {
            pace: Arc::clone(pace),
            tick,
        });
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
            .spawn((Client, RawClient, ReplicationReceiver))
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

/// The server's key pair, which signs what it logs.
pub(crate) fn server_keypair() -> Keypair {
    keypair(41)
}

pub(crate) fn server_key() -> XOnlyPublicKey {
    server_keypair().x_only_public_key().0
}

/// The test lane mode, its `[players]` as `rules` says, and its `[saves] by` `save_by`.
fn lane_mode(rules: PlayersData, save_by: SaveBy) -> ModePackages {
    let mut files = PackageDir::workspace_tree("test");
    let data = PathBuf::from("modes/lane/data/mode.toml");
    let mut text = String::from_utf8(files[&data].clone()).expect("the mode's data is UTF-8");
    let leaver = match rules.leaver {
        Leaver::Reserve => "reserve",
        Leaver::Bot => "bot",
        Leaver::Open => "open",
    };
    let by = match save_by {
        SaveBy::Player => "player",
        SaveBy::Mode => "mode",
    };
    write!(
        text,
        "\n[players]\nlate_join = {}\nbot_takeover = {}\nleaver = \"{leaver}\"\n\n[saves]\nby = \"{by}\"\n",
        rules.late_join, rules.bot_takeover
    )
    .expect("a String takes any text");
    files.insert(data, text.into_bytes());
    let dir = PackageDir::in_memory(Arc::new(files), "modes/lane");
    ModePackages::from_package_dir(&dir).expect("the test mode loads")
}
