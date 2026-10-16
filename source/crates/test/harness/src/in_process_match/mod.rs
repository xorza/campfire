use std::mem;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy_app::{App, First, PostUpdate, TaskPoolPlugin, Update};
use bevy_ecs::change_detection::Mut;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules, SingleThreadedExecutor};
use bevy_state::app::StatesPlugin;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use campfire_capabilities::{Action, Order};
use campfire_codec::Toml;
use campfire_common::{ChannelBinding, PlayerSlot, TlsKey};
use campfire_log::internals::LogCheck;
use campfire_mode::{PlayersData, SaveBy, SavesData};
use campfire_net::internals::{SimClientWorldInternals, SimServerWorldInternals};
use campfire_net::{
    BotScript, ClientDir, Clock, Hosting, JoinRole, JoinState, Lobby, LobbyOpening, LobbySetup,
    LocalPace, MatchClock, OrderScript, Os, Pace, PendingOrders, PendingRestore, ServerBots,
    ServerDir, ServerPin, ServerSetup, SessionDir, SessionTimes, SimClient, SimServerPlugins,
    SimServerWorld, SlotBot, TickHashes,
};
use campfire_package::ModePackages;
use campfire_package_disk::internals::{Edit, PackageTree, TestMode};
use campfire_protocol::internals::TestKey;
use campfire_protocol::{Entropy, InputRules, SeedChain, ServerInput};
use campfire_quic::{LinkBinding, QuicEndpoint};
use campfire_runner::Session;
use campfire_sim::{EntityIndex, SimTick, SimWorld, StableId, TickRate};
use campfire_units::internals::UnitsWorldInternals;
use campfire_units::{Body, MoveStep, Owner, UnitTypeBook};
use campfire_values::{NameId, Team};
use campfire_visibility::HostVisibility;
use lightyear::crossbeam::CrossbeamIo;
use lightyear::prelude::server::RawServer;
use lightyear::prelude::{
    Connect, Connected, LinkOf, LinkSystems, Linked, LocalTimelineSync, PeerAddr,
    PredictionManager, PredictionMetrics, RollbackMode, SyncSystems, Unlink, UnlinkReason,
};
use lightyear::transport::plugin::TransportSystems;

use crate::in_process_match::delay_line::DelayLine;
use crate::in_process_match::link_model::LinkModel;

pub(crate) mod delay_line;
pub(crate) mod link_model;

/// Frames a connection gets to link and sync its timeline, and a join to start the match.
const CONNECT_FRAMES: usize = 300;
/// In-process channels have no TLS; each client pins this as the server's TLS key.
const TLS_KEY: TlsKey = TlsKey::new([3; 32]);
/// In-process channels have no TLS session; both ends of a link take this binding.
const BINDING: LinkBinding = LinkBinding(ChannelBinding::new([3; 32]));

/// What an `InProcessMatch` runs: how many players join, and how.
#[derive(Debug, Clone, Copy)]
pub struct MatchSetup {
    /// The players, a client each, in the slots from 0: one or two in the lane mode, a slot a
    /// side, and at most `MAX_PLAYERS`.
    pub players: usize,
    /// How the server's links lie in its tables, which no result of the match may follow.
    pub links: LinkLayout,
    /// When each player's client rolls its state back, by player; those past the players go
    /// unused.
    pub rollbacks: [RollbackMode; Self::MAX_PLAYERS],
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
    /// The lane mode's `[saves]`.
    pub saves: SavesData,
    /// The order script of the server's bot in each slot after the players', in slot order.
    pub bots: &'static [&'static str],
    /// The script of the server's bot in a slot its player left, when it plays one.
    pub takeover: Option<&'static str>,
    /// The host's visibility settings, as `--visibility` gives them.
    pub visibility: &'static [&'static str],
    /// The most spectators the session admits.
    pub max_spectators: u32,
}

/// How an `InProcessMatch` lays the server's links out in its tables. A query gives entities in
/// the order of their tables, which Lightyear's parallel commands change from run to run once
/// Bevy's `multi_threaded` is on, so a match plays the same in both layouts when the server takes
/// nothing in a query's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkLayout {
    /// The clients connect in player order, and the links lie as Bevy puts them.
    InOrder,
    /// The clients connect in reverse, and before each server frame the links' rows go in the
    /// reverse of player order. The players take the same slots: one frame's joins go in the order
    /// of their main keys, and each player's sorts after the one before.
    Reversed,
}

/// A mark that moves a server link out of its table and back, so its row goes last.
#[derive(Component, Debug)]
struct Moved;

/// What a step cost each end: each client's frame, by client, and the server's worst frame.
#[derive(Debug, Clone, Default)]
pub(crate) struct StepCost {
    pub(crate) clients: Vec<Duration>,
    pub(crate) server: Duration,
}

impl StepCost {
    /// Clears it, for a step, or a bench's `add` and `keep_worst`, to fill.
    pub(crate) fn clear(&mut self) {
        self.clients.clear();
        self.server = Duration::ZERO;
    }
}

/// An app of a match: the server's, or a client's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Server,
    Client(usize),
}

impl MatchSetup {
    /// The most players a setup holds: the MOBA 3v3's six.
    pub const MAX_PLAYERS: usize = 6;

    /// One player through a perfect link, whose client rolls back only on a misprediction, with
    /// a server that runs a frame a tick.
    pub const SOLO: Self = Self::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN);

    /// One player, through perfect links.
    pub const fn solo(rollback: RollbackMode, server_frames: u32, seed_chain: SeedChain) -> Self {
        Self {
            players: 1,
            links: LinkLayout::InOrder,
            rollbacks: [rollback; Self::MAX_PLAYERS],
            server_frames,
            link: LinkModel::PERFECT,
            seed_chain,
            times: SessionTimes::DEFAULT,
            rules: PlayersData::DEFAULT,
            saves: SavesData {
                by: SaveBy::Player,
                autosave_ms: None,
            },
            bots: &[],
            takeover: None,
            visibility: &[],
            max_spectators: 0,
        }
    }

    /// Two players through `link`, whose clients roll back only on a misprediction, with a
    /// server that runs 3 frames a tick.
    pub const fn duo(link: LinkModel, seed_chain: SeedChain) -> Self {
        Self {
            players: 2,
            link,
            ..Self::solo(RollbackMode::Check, 3, seed_chain)
        }
    }
}

/// A server app and one client app for each player, joined by in-process channels through a
/// `LinkModel`, each on a manual clock that advances one tick per `step`, so every run repeats
/// exactly. Each player holds fixed keys and draws fixed random bytes, and the server keeps the
/// state hash after every tick.
#[derive(Debug)]
pub struct InProcessMatch {
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
    /// How many spectators it added, which takes each one's key.
    spectators: usize,
    setup: MatchSetup,
    packages: Arc<ModePackages>,
    /// The server's data directory, where it keeps its session, once a test gives it one.
    data: Option<PathBuf>,
    /// The pause and the speed every end follows.
    pace: Arc<Pace>,
    /// The Unix time every end's clock reads, `TestKey::NOW` until a test sets it.
    clock: Clock,
    /// What the last step cost each end, kept from step to step.
    step_cost: StepCost,
    /// The server's links that `lay_out_links` moves, in client order.
    link_rows: Vec<Entity>,
    /// Last, so it drops after the apps and sees what they log as they drop.
    log: LogCheck,
}

impl InProcessMatch {
    /// The seed chain the tests' servers commit to.
    pub const SEED_CHAIN: SeedChain = TestKey::seed_chain(NonZeroU32::MIN);

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
    pub fn new(setup: MatchSetup) -> Self {
        assert!(
            (1..=2).contains(&setup.players),
            "the lane mode takes 1 or 2 players"
        );
        Self::of_mode(setup, Self::lane_mode(setup.rules, setup.saves))
    }

    /// A match of `packages` at their default rate, its clients connected and synced.
    pub fn of_mode(setup: MatchSetup, packages: ModePackages) -> Self {
        let log = LogCheck::start();
        assert!(
            setup.players <= MatchSetup::MAX_PLAYERS,
            "a setup holds at most {} players",
            MatchSetup::MAX_PLAYERS
        );
        assert!(
            setup.server_frames > 0,
            "the server runs a frame a tick at least"
        );
        let packages = Arc::new(packages);
        let pace = Arc::new(Pace::default());
        let mut server = Self::server_app(&setup, &packages, &pace);
        // A raw server starts once linked, and in-process channels have no socket to link it.
        let server_entity = server.world_mut().spawn((RawServer, Linked)).id();
        server.finish();
        server.cleanup();
        run_in_order(&mut server);
        let mut local = Self {
            server,
            server_entity,
            clients: Vec::with_capacity(setup.players),
            client_entities: Vec::with_capacity(setup.players),
            links: Vec::with_capacity(setup.players),
            links_made: 0,
            spectators: 0,
            setup,
            packages,
            data: None,
            pace,
            clock: Clock::manual(TestKey::NOW),
            step_cost: StepCost::default(),
            link_rows: Vec::with_capacity(setup.players),
            log,
        };
        for client in 0..setup.players {
            let player = match setup.links {
                LinkLayout::InOrder => client,
                LinkLayout::Reversed => setup.players - 1 - client,
            };
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

    /// The server's app of `packages` at their default rate, its frames `setup.server_frames` a
    /// tick, before any link.
    fn server_app(setup: &MatchSetup, packages: &ModePackages, pace: &Arc<Pace>) -> App {
        let tick_hz = packages.manifest().tick_hz.default();
        let tick = TickRate::new(tick_hz).length();
        let mut server = App::new();
        server.add_plugins(SimServerPlugins {
            tick_hz,
            capabilities: packages.manifest().capabilities,
            hosting: Hosting::Local,
        });
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
        pass_through_delay_lines(&mut server);
        server
    }

    /// Adds a client of `player`, whose keys it holds, linked to the server and connecting; the
    /// index of the new client. A client of a player another client plays logs in again.
    pub fn add_client(&mut self, player: usize) -> usize {
        self.add(Joiner::Player(player))
    }

    /// Adds a client that joins to watch, with a key of its own, linked to the server and
    /// connecting; the index of the new client.
    pub fn add_spectator(&mut self) -> usize {
        let spectator = self.spectators;
        self.spectators += 1;
        self.add(Joiner::Spectator(spectator))
    }

    fn add(&mut self, joiner: Joiner) -> usize {
        let ClientApp { mut app, entity } = ClientApp::new(self, joiner);
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
                QuicEndpoint::link(),
                PeerAddr(address),
                BINDING,
                Linked,
                server_io,
                DelayLine::new(self.setup.link, self.setup.server_frames, stream, tick),
            ))
            .id();
        let entity = self.client_entities[client];
        self.clients[client].world_mut().entity_mut(entity).insert((
            client_io,
            BINDING,
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
        self.clock.set(seconds);
    }

    /// Every input the server logged, in the order logged.
    pub fn server_inputs(&self) -> Vec<ServerInput<'_>> {
        let session = self.server.world().resource::<Session>();
        session.log().server_inputs().collect()
    }

    /// The test lane mode, its `[players]` as `rules` says, and its `[saves]` as `saves` says.
    pub fn lane_mode(rules: PlayersData, saves: SavesData) -> ModePackages {
        let added = format!(
            "\n[players]\n{}\n[saves]\n{}",
            Toml::write(&rules),
            Toml::write(&saves)
        );
        PackageTree::of(TestMode::Lane)
            .edit("modes/lane/data/mode.toml", Edit::Append(&added))
            .packages()
    }

    /// What the server opens or restores its session with: its key, the setup's times, a fixed
    /// clock and fixed random bytes.
    fn server_setup(&self) -> ServerSetup {
        ServerSetup {
            key: TestKey::server(),
            times: self.setup.times,
            os: Os {
                clock: self.clock.clone(),
                entropy: Entropy::Fixed(5),
            },
        }
    }

    /// The bots of the setup: its bots in the slots after the players', and its takeover bot.
    fn server_bots(&self) -> ServerBots {
        let script = |text| OrderScript::parse(text).expect("a test's bot script reads");
        let slots = (self.setup.players..)
            .zip(self.setup.bots)
            .map(|(slot, &text)| SlotBot {
                slot: PlayerSlot::new(u32::try_from(slot).expect("a small count")),
                script: script(text),
            });
        ServerBots {
            slots: slots.collect(),
            takeover: self.setup.takeover.map(script),
        }
    }

    /// Gives the server the data directory `dir`, which it holds locked while it runs, where the
    /// session `start_match` opens keeps its directory, private record and journal, and each
    /// client `client-<index>` in it, where it writes its receipts.
    pub fn keep_data(&mut self, dir: PathBuf) {
        assert!(
            !self.server.world().contains_resource::<MatchClock>(),
            "a server keeps its data from before the match"
        );
        for (client, app) in self.clients.iter_mut().enumerate() {
            let own = ClientDir::open(&dir.join(format!("client-{client}"))).unwrap();
            app.world_mut().keep_receipts(Arc::new(own));
        }
        let data = ServerDir::open(&dir).unwrap();
        self.server.world_mut().insert_resource(data);
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
    /// its journal holds in its first frame; see `PendingRestore`. Each client gets a new link to
    /// it, which it connects through once its wait passed.
    pub fn restart_server(&mut self) {
        self.stop_server();
        let path = self.data.as_ref().expect("a server with a data directory");
        let data = ServerDir::open(path).unwrap();
        let dir = SessionDir::find(&data)
            .unwrap()
            .expect("a session the stop ended");
        let session = dir
            .restore()
            .unwrap()
            .expect("a session whose match started");
        let mut server = Self::server_app(&self.setup, &self.packages, &self.pace);
        self.server_entity = server.world_mut().spawn((RawServer, Linked)).id();
        server.finish();
        server.cleanup();
        run_in_order(&mut server);
        server.world_mut().insert_resource(data);
        server.world_mut().insert_resource(PendingRestore {
            session,
            packages: Arc::clone(&self.packages),
            server: self.server_setup(),
            bots: self.server_bots(),
        });
        server.update();
        assert!(
            server.world().contains_resource::<MatchClock>(),
            "the session restores in the server's first frame"
        );
        self.server = server;
        for client in 0..self.clients.len() {
            self.mend_link(client);
        }
    }

    /// `open_match`, then `await_avatars` for `CONNECT_FRAMES`, as the lane mode's avatars come
    /// with its first tick. A client runs match ticks from the match start's message, and its
    /// avatar comes in the replication after the server's first tick, in another packet: which
    /// arrives first varies with how Lightyear packs and resends them, by the wall clock.
    pub fn start_match(&mut self) {
        self.open_match();
        self.await_avatars(CONNECT_FRAMES);
    }

    /// Opens the session for every client, and steps until each joined and every end runs the
    /// match; see `Lobby`. The players take the slots in player order, as `LinkLayout` says.
    pub fn open_match(&mut self) {
        let packages = Arc::clone(&self.packages);
        let opening = LobbyOpening {
            packages: Arc::clone(&packages),
            bots: self.server_bots(),
            open: Vec::new(),
            host: HostVisibility {
                settings: self
                    .setup
                    .visibility
                    .iter()
                    .map(|text| text.parse().expect("a host setting"))
                    .collect(),
                max_spectators: self.setup.max_spectators,
            },
            server: self.server_setup(),
        };
        let mut lobby = Lobby::new(LobbySetup {
            opening,
            tick_hz: packages.manifest().tick_hz.default(),
            seed_chain: self.setup.seed_chain,
            inputs: InputRules::LAN,
            slots: self.setup.players + self.setup.bots.len(),
        })
        .expect("the lane mode runs at its default rate");
        if let Some(data) = self.server.world().get_resource::<ServerDir>() {
            lobby.keep_files(data).unwrap();
        }
        self.server.world_mut().insert_resource(lobby);
        for _ in 0..CONNECT_FRAMES {
            let playing = |client: &App| {
                client
                    .world()
                    .resource::<JoinState>()
                    .playing_clock()
                    .is_some()
            };
            if self.server.world().contains_resource::<MatchClock>()
                && self.clients.iter().all(playing)
            {
                return;
            }
            self.step();
        }
        panic!("the match did not start in {CONNECT_FRAMES} frames");
    }

    /// Steps until each player's client holds its player's avatar, for at most `frames` frames:
    /// a mode may spawn the avatars only after its pick.
    pub fn await_avatars(&mut self, frames: usize) {
        for _ in 0..frames {
            if (0..self.setup.players).all(|client| self.holds_avatar(client)) {
                return;
            }
            self.step();
        }
        panic!("the clients did not hold their avatars in {frames} frames");
    }

    /// Whether `client` holds its player's avatar, which the server spawned.
    fn holds_avatar(&self, client: usize) -> bool {
        let world = self.clients[client].world();
        self.spawned_avatar(client)
            .is_some_and(|avatar| world.entity_of(avatar).is_some())
    }

    /// Loses every packet the server sends `client` from now on, as a link that drops one way
    /// does, though the client's own packets arrive.
    pub fn lose_server_packets(&mut self, client: usize) {
        self.server_delay(client).lose_all();
    }

    /// Passes the packets the server sends `client` again.
    pub fn pass_server_packets(&mut self, client: usize) {
        self.server_delay(client).pass_all();
    }

    /// The delay line of the server's link to `client`.
    fn server_delay(&mut self, client: usize) -> Mut<'_, DelayLine> {
        let world = self.server.world_mut();
        let delay = world.get_mut::<DelayLine>(self.links[client]);
        delay.expect("a link passes through a delay line")
    }

    /// One frame of the server alone, which shifts where in a step its ticks fall.
    pub fn server_frame(&mut self) {
        self.lay_out_links();
        self.server.update();
    }

    /// Puts the rows of the server's links in each of their tables in client order, the reverse of
    /// player order, when the setup lays them out reversed: all go out to another table, then
    /// back in client order, as Bevy appends a row that enters a table.
    fn lay_out_links(&mut self) {
        if self.setup.links == LinkLayout::InOrder {
            return;
        }
        let world = self.server.world_mut();
        self.link_rows.clear();
        self.link_rows.extend(
            self.links
                .iter()
                .copied()
                .filter(|&link| world.get::<LinkOf>(link).is_some()),
        );
        for &link in &self.link_rows {
            world.entity_mut(link).insert(Moved);
        }
        for &link in &self.link_rows {
            world.entity_mut(link).remove::<Moved>();
        }
    }

    /// One frame of each client, then the server's frames: one tick each, what each end's frames
    /// cost kept in `step_cost`. A server whose session went back to a save then starts again on
    /// its data, as a local server does.
    pub fn step(&mut self) {
        self.step_cost.clear();
        for client in 0..self.clients.len() {
            let start = Instant::now();
            self.client_frame(client);
            self.step_cost.clients.push(start.elapsed());
        }
        for _ in 0..self.setup.server_frames {
            let start = Instant::now();
            self.server_frame();
            self.step_cost.server = self.step_cost.server.max(start.elapsed());
        }
        if self.server.world().reload_wanted() {
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
    pub fn order(&mut self, client: usize, action: impl Into<Action>) {
        let unit = self.avatar(client);
        self.clients[client]
            .world_mut()
            .resource_mut::<PendingOrders>()
            .push(Order::one(unit, action));
    }

    /// Makes `client` play `script`, as a bot does.
    pub fn play(&mut self, client: usize, script: &OrderScript) {
        let world = self.clients[client].world_mut();
        let types = world.resource::<UnitTypeBook>().types();
        let bot = BotScript::new(script, types).expect("a test's bot names its mode's types");
        world.insert_resource(bot);
    }

    /// `client`'s player's avatar, as the server holds it.
    pub fn avatar(&self, client: usize) -> StableId {
        self.spawned_avatar(client)
            .expect("the match started, with the player's avatar")
    }

    /// `client`'s player's avatar, as the server holds it, once the server spawned it.
    fn spawned_avatar(&self, client: usize) -> Option<StableId> {
        let world = self.server.world();
        let slot = world
            .link_slot(self.links[client])
            .expect("the match started, with the client's slot");
        world.avatar_of(slot).map(|avatar| avatar.id)
    }

    /// Makes each client play the script of its avatar's team, by team index, as a bot does; gives
    /// each client's team index, which follows the client's player, not its index, under
    /// `LinkLayout::Reversed`.
    pub fn play_by_team(&mut self, scripts: [&str; 2]) -> [usize; 2] {
        let teams = [0, 1].map(|client| self.team(client).index());
        assert_ne!(
            teams[0], teams[1],
            "the two players' avatars are on two teams"
        );
        for (client, team) in teams.into_iter().enumerate() {
            let script = OrderScript::parse(scripts[team]).expect("a scenario's script reads");
            self.play(client, &script);
        }
        teams
    }

    /// The team of `client`'s player's avatar.
    pub fn team(&self, client: usize) -> Team {
        let world = self.server.world();
        let avatar = world.entity_of(self.avatar(client));
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

    /// Makes the server give its receipts in its next update, as if their second passed.
    pub fn give_receipts(&mut self) {
        self.server.world_mut().give_receipts();
    }

    /// Holds the server's next round of receipts until `give_receipts`, so a test that steps
    /// slower than real time, as one does under a loaded host, meets none it did not ask for.
    pub fn hold_receipts(&mut self) {
        self.server.world_mut().hold_receipts();
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

/// A client app of an `InProcessMatch`, and its client entity.
#[derive(Debug)]
struct ClientApp {
    app: App,
    entity: Entity,
}

/// Who a client joins as: a player, whose keys and rollback mode the setup gives, or the
/// `n`th spectator, with a key of its own.
#[derive(Debug, Clone, Copy)]
enum Joiner {
    Player(usize),
    Spectator(usize),
}

impl ClientApp {
    /// The app of `joiner` in `local`, at its packages' default rate, following its pace and
    /// reading its clock.
    fn new(local: &InProcessMatch, joiner: Joiner) -> Self {
        let setup = &local.setup;
        let tick_hz = local.packages.manifest().tick_hz.default();
        let (keys, role, rollback) = match joiner {
            Joiner::Player(player) => (
                TestKey::player(u32::try_from(player).unwrap()),
                JoinRole::Play,
                setup.rollbacks[player],
            ),
            Joiner::Spectator(spectator) => (
                TestKey::spectator(u32::try_from(spectator).unwrap()),
                JoinRole::Watch,
                RollbackMode::Check,
            ),
        };
        let sim_client = SimClient {
            main_key: keys.main,
            session_key: keys.session,
            server: ServerPin {
                key: TestKey::server().x_only_public_key().0,
                tls_key: TLS_KEY,
                tick_hz,
            },
            role,
            hosting: Hosting::Local,
            packages: Arc::clone(&local.packages),
            os: Os {
                clock: local.clock.clone(),
                entropy: Entropy::Fixed(4),
            },
            data: None,
        };
        let tick = TickRate::new(tick_hz).length();
        let mut client = App::new();
        client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        client.add_plugins(sim_client);
        client.add_plugins(LocalPace {
            pace: Arc::clone(&local.pace),
            tick,
        });
        client.insert_resource(TimeUpdateStrategy::ManualDuration(tick));
        client
            .world_mut()
            .resource_mut::<PredictionManager>()
            .rollback_policy
            .state = rollback;
        pass_through_delay_lines(&mut client);
        let entity = SimClient::spawn(client.world_mut());
        Self {
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

pub(crate) mod bench {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use std::{env, process};

    use campfire_math::{Num, Vec3};
    use campfire_orders::OrdersAction;
    use campfire_package_disk::internals::TestMode;
    use campfire_sim::{Position, SimWorld};
    use campfire_store::DurableDir;
    use lightyear::prelude::RollbackMode;

    use crate::in_process_match::link_model::LinkModel;
    use crate::in_process_match::{CONNECT_FRAMES, End, InProcessMatch, MatchSetup, StepCost};
    use campfire_net::OrderScript;

    /// A quarter meter a tick crosses the 10 m between the two targets in 40 ticks, so a new
    /// order every 40 frames keeps the avatar walking and the server sending updates.
    const LEG_FRAMES: u64 = 40;
    /// The ticks of the lane 1v1 a worst case plays.
    const MATCH_TICKS: u64 = 600;

    /// The data directories the benches of this process made, each a new one.
    static DATA_DIRS: AtomicU64 = AtomicU64::new(0);

    /// How each player's client of the walking 3v3 rolls back: only on a misprediction, on
    /// every confirmed update, and never.
    const ROLLBACKS_3V3: [RollbackMode; 3] = [
        RollbackMode::Check,
        RollbackMode::Always,
        RollbackMode::Disabled,
    ];
    /// The MOBA 3v3's first wave at its 30 Hz: 60 s of pick, then 60 s.
    const FIRST_WAVE_3V3: u64 = 3600;
    /// The pick of each slot of the MOBA 3v3, the players' then the bots': a hero each,
    /// none twice, in tick 1, and the two spells in tick 2, as `Moba3v3`'s slots pick.
    static PICKS_3V3: [&str; 6] = [
        "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-cinder\"\n\
         [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
        "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-gale\"\n\
         [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
        "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-husk\"\n\
         [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
        "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-kensho\"\n\
         [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
        "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-rime\"\n\
         [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
        "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-veil\"\n\
         [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    ];

    impl StepCost {
        /// Adds each end's cost in `step` to its own.
        fn add(&mut self, step: &Self) {
            self.clients.resize(step.clients.len(), Duration::ZERO);
            for (own, &spent) in self.clients.iter_mut().zip(&step.clients) {
                *own += spent;
            }
            self.server += step.server;
        }

        /// Keeps each end's worse of its own and its cost in `step`.
        fn keep_worst(&mut self, step: &Self) {
            self.clients.resize(step.clients.len(), Duration::ZERO);
            for (own, &spent) in self.clients.iter_mut().zip(&step.clients) {
                *own = (*own).max(spent);
            }
            self.server = self.server.max(step.server);
        }

        /// The worst of the clients' costs.
        pub(crate) fn worst_client(&self) -> Duration {
            let worst = self.clients.iter().copied().max();
            worst.expect("a cost of a match with clients")
        }
    }

    impl InProcessMatch {
        /// A solo match whose client rolls back as `rollback` says, started, for `walk_steps`.
        pub(crate) fn walking(rollback: RollbackMode) -> Self {
            let mut local = Self::new(MatchSetup::solo(rollback, 1, Self::SEED_CHAIN));
            local.start_match();
            local
        }

        /// The MOBA 3v3 at its default rate, a server frame a tick, with a player in each
        /// of the first slots, whose clients roll back as `ROLLBACKS_3V3` says, and the
        /// server's bots in the others, every slot picking as `PICKS_3V3` says; played to its
        /// first wave, each client holding its hero, for `walk_steps`.
        pub(crate) fn walking_3v3() -> Self {
            let players = ROLLBACKS_3V3.len();
            let mut setup = MatchSetup::solo(RollbackMode::Check, 1, Self::SEED_CHAIN);
            setup.players = players;
            setup.rollbacks[..players].copy_from_slice(&ROLLBACKS_3V3);
            setup.bots = &PICKS_3V3[players..];
            let packages = TestMode::Moba3v3.packages();
            let mut local = Self::of_mode(setup, packages);
            local.open_match();
            for (client, pick) in PICKS_3V3[..players].iter().enumerate() {
                local.play(client, &OrderScript::parse(pick).expect("a pick reads"));
            }
            while local.next_tick(End::Server) < FIRST_WAVE_3V3 {
                local.step();
            }
            local.await_avatars(CONNECT_FRAMES);
            local
        }

        /// Where each client's avatar stands on the server, by client, into `places`.
        pub(crate) fn avatar_places(&self, places: &mut Vec<Vec3>) {
            places.clear();
            let world = self.server.world();
            for client in 0..self.clients.len() {
                let avatar = world.entity_of(self.avatar(client));
                let pos = world.get::<Position>(avatar.expect("the avatar exists"));
                places.push(pos.expect("an avatar has a place").get());
            }
        }

        /// `steps` frames of a walk from frame `*frame` on, which it advances: every
        /// `LEG_FRAMES` frames, each client's avatar ordered to the target 5 m on the other side
        /// of its point in `around`, along z; each end's frames' cost added to `spent`.
        pub(crate) fn walk_steps(
            &mut self,
            frame: &mut u64,
            steps: u64,
            around: &[Vec3],
            spent: &mut StepCost,
        ) {
            debug_assert_eq!(around.len(), self.clients.len());
            for _ in 0..steps {
                if frame.is_multiple_of(LEG_FRAMES) {
                    let side = if frame.is_multiple_of(2 * LEG_FRAMES) {
                        5
                    } else {
                        -5
                    };
                    let side = Num::from_int(side).expect("a small integer");
                    for (client, point) in around.iter().enumerate() {
                        let (x, z) = (point.x, point.z + side);
                        self.order(client, OrdersAction::Move { x, z });
                    }
                }
                self.step();
                spent.add(&self.step_cost);
                *frame += 1;
            }
        }

        /// Each end's worst step of `MATCH_TICKS` ticks of the lane 1v1, as the match scenario
        /// plays it: the rollback of each avatar's death falls in the clients'. The server and
        /// the clients keep their data, in a new directory under the system's temporary one, so
        /// the journal and the receipts write as a real match's do; it is removed after.
        pub(crate) fn worst_1v1() -> StepCost {
            let dir = env::temp_dir().join(format!(
                "campfire-bench-{}-{}",
                process::id(),
                DATA_DIRS.fetch_add(1, Ordering::Relaxed)
            ));
            let mut local = Self::new(MatchSetup::duo(LinkModel::PERFECT, Self::SEED_CHAIN));
            local.keep_data(dir.clone());
            local.start_match();
            local.play_by_team(Self::SCENARIO_SCRIPTS);
            let mut worst = StepCost::default();
            for _ in 0..MATCH_TICKS {
                local.step();
                worst.keep_worst(&local.step_cost);
            }
            drop(local);
            DurableDir::remove(&dir).expect("the bench's own data directory");
            worst
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_step_cost_adds_and_keeps_the_worst_of_each_end() {
            let micros = Duration::from_micros;
            let step = |clients: &[u64], server: u64| StepCost {
                clients: clients.iter().map(|&each| micros(each)).collect(),
                server: micros(server),
            };
            let steps = [step(&[3, 9], 5), step(&[7, 2], 4)];
            let (mut total, mut worst) = (StepCost::default(), StepCost::default());
            for each in &steps {
                total.add(each);
                worst.keep_worst(each);
            }
            // Each end on its own: client 0 took 3 and 7, client 1 9 and 2, the server 5 and 4.
            assert_eq!(total.clients, [micros(10), micros(11)]);
            assert_eq!(total.server, micros(9));
            assert_eq!(worst.clients, [micros(7), micros(9)]);
            assert_eq!(worst.server, micros(5));
            assert_eq!(worst.worst_client(), micros(9));
            total.clear();
            assert_eq!((total.clients.len(), total.server), (0, Duration::ZERO));
        }
    }
}
