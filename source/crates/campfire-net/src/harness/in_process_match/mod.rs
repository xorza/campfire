use std::array;
use std::cell::Cell;
use std::fmt::Write as _;
use std::mem;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy_app::{App, First, PostUpdate, TaskPoolPlugin, Update};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules, SingleThreadedExecutor};
use bevy_state::app::StatesPlugin;
use bevy_time::{TimePlugin, TimeUpdateStrategy};
use campfire_capabilities::{
    Action, Body, CapabilitySet, Leaver, MoveStep, Order, Owner, PlayersData, SaveBy, SavesData,
    Team,
};
use campfire_common::{MapName, PlayerSlot};
use campfire_log::internals::LogCheck;
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::internals::TestKey;
use campfire_protocol::{CertificateHash, SeedChain, ServerInput, SessionPrivate};
use campfire_runner::{InputRules, Session};
use campfire_sim::{EntityIndex, SimTick, StableId, TickRate};
use lightyear::crossbeam::CrossbeamIo;
use lightyear::prelude::server::RawServer;
use lightyear::prelude::{
    Connect, Connected, Link, LinkOf, LinkSystems, Linked, LocalTimelineSync, PeerAddr,
    PredictionManager, PredictionMetrics, RollbackMode, SyncSystems, Unlink, UnlinkReason,
};
use lightyear::transport::plugin::TransportSystems;

use crate::bot_script::BotScript;
use crate::harness::in_process_match::delay_line::DelayLine;
use crate::harness::in_process_match::link_model::LinkModel;
use crate::local::local_pace::LocalPace;
use crate::local::local_session::LocalSession;
use crate::match_clock::MatchClock;
use crate::order_script::OrderScript;
use crate::pace::Pace;
use crate::session_times::SessionTimes;
use crate::sim_client::client_dir::ClientDir;
use crate::sim_client::join_state::JoinState;
use crate::sim_client::receipt_writer::ReceiptWriter;
use crate::sim_client::server_pin::ServerPin;
use crate::sim_client::{PendingOrders, SimClient};
use crate::sim_server::SimServer;
use crate::sim_server::lobby::{Lobby, LobbySetup};
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::receipts::Receipts;
use crate::sim_server::server_bots::{ServerBots, SlotBot};
use crate::sim_server::server_dir::ServerDir;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::session_dir::SessionDir;
use crate::sim_server::tick_hashes::TickHashes;

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
    pub rollbacks: [RollbackMode; MatchSetup::MAX_PLAYERS],
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
    pub const SOLO: MatchSetup =
        MatchSetup::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN);

    /// One player, through perfect links.
    pub const fn solo(
        rollback: RollbackMode,
        server_frames: u32,
        seed_chain: SeedChain,
    ) -> MatchSetup {
        MatchSetup {
            players: 1,
            links: LinkLayout::InOrder,
            rollbacks: [rollback; MatchSetup::MAX_PLAYERS],
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
        }
    }

    /// Two players through `link`, whose clients roll back only on a misprediction, with a
    /// server that runs 3 frames a tick.
    pub const fn duo(link: LinkModel, seed_chain: SeedChain) -> MatchSetup {
        MatchSetup {
            players: 2,
            links: LinkLayout::InOrder,
            rollbacks: [RollbackMode::Check; MatchSetup::MAX_PLAYERS],
            server_frames: 3,
            link,
            seed_chain,
            times: SessionTimes::DEFAULT,
            rules: PlayersData::DEFAULT,
            saves: SavesData {
                by: SaveBy::Player,
                autosave_ms: None,
            },
            bots: &[],
            takeover: None,
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
    setup: MatchSetup,
    packages: Arc<ModePackages>,
    /// The server's data directory, where it keeps its session, once a test gives it one.
    data: Option<PathBuf>,
    /// The pause and the speed every end follows.
    pace: Arc<Pace>,
    /// What the last step cost each end, kept from step to step.
    step_cost: StepCost,
    /// The server's links that `lay_out_links` moves, in client order.
    link_rows: Vec<Entity>,
    /// Last, so it drops after the apps and sees what they log as they drop.
    log: LogCheck,
}

impl InProcessMatch {
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
    pub fn new(setup: MatchSetup) -> InProcessMatch {
        assert!(
            (1..=2).contains(&setup.players),
            "the lane mode takes 1 or 2 players"
        );
        InProcessMatch::of_mode(setup, InProcessMatch::lane_mode(setup.rules, setup.saves))
    }

    /// A match of `packages` at their default rate, its clients connected and synced.
    pub(crate) fn of_mode(setup: MatchSetup, packages: ModePackages) -> InProcessMatch {
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
        let tick_hz = packages.manifest().tick_hz.default();
        let tick = TickRate::new(tick_hz).length();

        let pace = Arc::new(Pace::default());
        let capabilities = packages.manifest().capabilities;
        let mut server = InProcessMatch::server_app(&setup, capabilities, tick, &pace);
        // A raw server starts once linked, and in-process channels have no socket to link it.
        let server_entity = server.world_mut().spawn((RawServer, Linked)).id();
        server.finish();
        server.cleanup();
        run_in_order(&mut server);
        let mut local = InProcessMatch {
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

    /// The server's app, its frames `setup.server_frames` a tick of `tick`, before any link.
    fn server_app(
        setup: &MatchSetup,
        capabilities: CapabilitySet,
        tick: Duration,
        pace: &Arc<Pace>,
    ) -> App {
        let mut server = App::new();
        server.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        server.add_plugins(SimServer { tick, capabilities });
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
    pub fn server_inputs(&self) -> Vec<ServerInput<'_>> {
        let session = self.server.world().resource::<Session>();
        session.log().server_inputs().collect()
    }

    /// The test lane mode, its `[players]` as `rules` says, and its `[saves]` as `saves` says.
    pub(crate) fn lane_mode(rules: PlayersData, saves: SavesData) -> ModePackages {
        let mut files = PackageDir::workspace_tree("test");
        let data = PathBuf::from("modes/lane/data/mode.toml");
        let mut text = String::from_utf8(files[&data].clone()).expect("the mode's data is UTF-8");
        let leaver = match rules.leaver {
            Leaver::Reserve => "reserve",
            Leaver::Bot => "bot",
            Leaver::Open => "open",
        };
        let by = match saves.by {
            SaveBy::Player => "player",
            SaveBy::Mode => "mode",
        };
        write!(
            text,
            "\n[players]\nlate_join = {}\nbot_takeover = {}\nleaver = \"{leaver}\"\n\n[saves]\nby = \"{by}\"\n",
            rules.late_join, rules.bot_takeover
        )
        .expect("a String takes any text");
        if let Some(every) = saves.autosave_ms {
            writeln!(text, "autosave_ms = {every}").expect("a String takes any text");
        }
        files.insert(data, text.into_bytes());
        let dir = PackageDir::in_memory(Arc::new(PackageDir::reindexed(files)), "modes/lane");
        ModePackages::from_package_dir(&dir, &MapName::new("lane").expect("a map's name"))
            .expect("the test mode loads")
    }

    /// What the server opens or restores its session with: its key, the certificate hash both
    /// ends take, the setup's times, a fixed clock and fixed random bytes.
    fn server_setup(&self) -> ServerSetup {
        ServerSetup {
            key: TestKey::server(),
            certificate: CERTIFICATE,
            times: self.setup.times,
            clock: unix_now,
            entropy: |bytes| bytes.fill(5),
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
            app.world_mut()
                .insert_resource(ReceiptWriter::start(Arc::new(own)));
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
    /// its journal holds; see `SimServer::restore_match`. Each client gets a new link to it, which
    /// it connects through once its wait passed.
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
        let tick = TickRate::new(self.packages.manifest().tick_hz.default()).length();
        let capabilities = self.packages.manifest().capabilities;
        let mut server = InProcessMatch::server_app(&self.setup, capabilities, tick, &self.pace);
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
        .unwrap();
        server.world_mut().insert_resource(data);
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
    pub(crate) fn open_match(&mut self) {
        let packages = Arc::clone(&self.packages);
        let mut lobby = Lobby::new(LobbySetup {
            tick_hz: packages.manifest().tick_hz.default(),
            packages,
            seed_chain: self.setup.seed_chain,
            inputs: InputRules::LAN,
            slots: self.setup.players + self.setup.bots.len(),
            bots: self.server_bots(),
            open: Vec::new(),
            server: self.server_setup(),
        })
        .expect("the lane mode runs at its default rate");
        if let Some(data) = self.server.world().get_resource::<ServerDir>() {
            let private = SessionPrivate {
                seed_chain: self.setup.seed_chain,
                terms: lobby.terms().clone(),
            };
            let dir = SessionDir::create(data, &private).unwrap();
            let files = dir.start().unwrap();
            lobby.keep_files(files);
        }
        self.server.world_mut().insert_resource(lobby);
        for _ in 0..CONNECT_FRAMES {
            let playing = |client: &App| client.world().resource::<JoinState>().clock().is_some();
            if self.server.world().contains_resource::<MatchClock>()
                && self.clients.iter().all(playing)
            {
                return;
            }
            self.step();
        }
        panic!("the match did not start in {CONNECT_FRAMES} frames");
    }

    /// Steps until each client holds its player's avatar, for at most `frames` frames: a mode
    /// may spawn the avatars only after its pick.
    pub(crate) fn await_avatars(&mut self, frames: usize) {
        for _ in 0..frames {
            if (0..self.clients.len()).all(|client| self.holds_avatar(client)) {
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
            .is_some_and(|avatar| world.resource::<EntityIndex>().get(avatar).is_some())
    }

    /// Loses every packet the server sends `client` while `lost`, as a link that drops one way
    /// does, though the client's own packets arrive; with `false`, passes them again.
    pub fn lose_server_packets(&mut self, client: usize, lost: bool) {
        let world = self.server.world_mut();
        let mut delay = world.get_mut::<DelayLine>(self.links[client]);
        delay
            .as_mut()
            .expect("a link passes through a delay line")
            .lose_all(lost);
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

    /// One frame of each client, then the server's frames: one tick each. A server whose session
    /// went back to a save then starts again on its data, as a local server does.
    pub fn step(&mut self) {
        self.timed_step();
    }

    /// `step`, which keeps what each end's frames cost in `step_cost`.
    pub(crate) fn timed_step(&mut self) {
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
            .push(Order::one(unit, action));
    }

    /// Makes `client` play `script`, as a bot does.
    pub fn play(&mut self, client: usize, script: OrderScript) {
        self.clients[client]
            .world_mut()
            .insert_resource(BotScript::new(script));
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
            self.play(client, script);
        }
        teams
    }

    /// The team of `client`'s player's avatar.
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

    /// Makes the server give its receipts in its next update, as if their second passed.
    pub fn give_receipts(&mut self) {
        self.server.world_mut().resource_mut::<Receipts>().due_now();
    }

    /// Holds the server's next round of receipts until `give_receipts`, so a test that steps
    /// slower than real time, as one does under a loaded host, meets none it did not ask for.
    pub fn hold_receipts(&mut self) {
        self.server.world_mut().resource_mut::<Receipts>().hold();
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

impl ClientApp {
    /// The secret of `player`'s main key, which sorts after every lower player's: the session
    /// key's is the next byte, so the odd bytes are the main keys'.
    fn main_secret(player: usize) -> u8 {
        let mut secrets: [u8; MatchSetup::MAX_PLAYERS] =
            array::from_fn(|player| u8::try_from(2 * player + 1).expect("a small player"));
        secrets.sort_by_key(|&secret| TestKey::of(secret).x_only_public_key().0.serialize());
        secrets[player]
    }

    /// The app of `player`, following `pace`.
    fn new(
        setup: &MatchSetup,
        player: usize,
        packages: &Arc<ModePackages>,
        tick_hz: NonZeroU32,
        pace: &Arc<Pace>,
    ) -> ClientApp {
        let secret = ClientApp::main_secret(player);
        let sim_client = SimClient {
            main_key: TestKey::of(secret),
            session_key: TestKey::of(secret + 1),
            server: ServerPin {
                key: TestKey::server().x_only_public_key().0,
                certificate: CERTIFICATE,
                tick_hz,
            },
            local: true,
            packages: Arc::clone(packages),
            clock: unix_now,
            entropy: |bytes| bytes.fill(4),
            data: None,
        };
        let tick = TickRate::new(tick_hz).length();
        let mut client = App::new();
        client.add_plugins((TaskPoolPlugin::default(), TimePlugin, StatesPlugin));
        client.add_plugins(sim_client);
        client.add_plugins(LocalPace {
            pace: Arc::clone(pace),
            tick,
        });
        client.insert_resource(TimeUpdateStrategy::ManualDuration(tick));
        client
            .world_mut()
            .resource_mut::<PredictionManager>()
            .rollback_policy
            .state = setup.rollbacks[player];
        pass_through_delay_lines(&mut client);
        let entity = SimClient::spawn_client(client.world_mut());
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

#[cfg(feature = "bench")]
pub(crate) mod bench {
    use std::env;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use campfire_capabilities::Action;
    use campfire_common::MapName;
    use campfire_math::{Num, Vec3};
    use campfire_package::{ModePackages, PackageDir};
    use campfire_sim::{EntityIndex, Position};
    use campfire_store::DurableFile;
    use lightyear::prelude::RollbackMode;

    use crate::harness::in_process_match::link_model::LinkModel;
    use crate::harness::in_process_match::{
        CONNECT_FRAMES, End, InProcessMatch, MatchSetup, StepCost,
    };
    use crate::order_script::OrderScript;

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
        fn add(&mut self, step: &StepCost) {
            self.clients.resize(step.clients.len(), Duration::ZERO);
            for (own, &spent) in self.clients.iter_mut().zip(&step.clients) {
                *own += spent;
            }
            self.server += step.server;
        }

        /// Keeps each end's worse of its own and its cost in `step`.
        fn keep_worst(&mut self, step: &StepCost) {
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
        pub(crate) fn walking(rollback: RollbackMode) -> InProcessMatch {
            let mut local =
                InProcessMatch::new(MatchSetup::solo(rollback, 1, InProcessMatch::SEED_CHAIN));
            local.start_match();
            local
        }

        /// The MOBA 3v3 at its default rate, a server frame a tick, with a player in each
        /// of the first slots, whose clients roll back as `ROLLBACKS_3V3` says, and the
        /// server's bots in the others, every slot picking as `PICKS_3V3` says; played to its
        /// first wave, each client holding its hero, for `walk_steps`.
        pub(crate) fn walking_3v3() -> InProcessMatch {
            let players = ROLLBACKS_3V3.len();
            let mut setup = MatchSetup::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN);
            setup.players = players;
            setup.rollbacks[..players].copy_from_slice(&ROLLBACKS_3V3);
            setup.bots = &PICKS_3V3[players..];
            let packages = ModePackages::from_dir(
                &PackageDir::workspace("test/moba/modes/3v3"),
                &MapName::new("two_lanes").expect("a map's name"),
            )
            .expect("the MOBA 3v3 loads");
            let mut local = InProcessMatch::of_mode(setup, packages);
            local.open_match();
            for (client, pick) in PICKS_3V3[..players].iter().enumerate() {
                local.play(client, OrderScript::parse(pick).expect("a pick reads"));
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
                let avatar = world.resource::<EntityIndex>().get(self.avatar(client));
                let at = world.get::<Position>(avatar.expect("the avatar exists"));
                places.push(at.expect("an avatar has a place").get());
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
                        self.order(client, Action::Move { x, z });
                    }
                }
                self.timed_step();
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
            let mut local = InProcessMatch::new(MatchSetup::duo(
                LinkModel::PERFECT,
                InProcessMatch::SEED_CHAIN,
            ));
            local.keep_data(dir.clone());
            local.start_match();
            local.play_by_team(InProcessMatch::SCENARIO_SCRIPTS);
            let mut worst = StepCost::default();
            for _ in 0..MATCH_TICKS {
                local.timed_step();
                worst.keep_worst(&local.step_cost);
            }
            drop(local);
            DurableFile::remove_dir(&dir).expect("the bench's own data directory");
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
