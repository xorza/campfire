use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bevy_app::App;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use bevy_time::{Time, Virtual};
use campfire_bots::{BotNames, BotScript, OrderScript};
use campfire_capabilities::{Directive, Order};
use campfire_common::{Seat, Tick, TlsKey};
use campfire_harness::HashTrail;
use campfire_log::internals::LogCheck;
use campfire_mode::{PlayersData, SaveBy, SavesData};
use campfire_net::internals::TickHashes;
use campfire_net::internals::{SimClientWorldInternals, SimServerWorldInternals};
use campfire_net::{
    ClientDir, JoinState, Lobby, LobbyOpening, LobbySetup, MatchClock, Os, Pace, PendingOrders,
    PendingRestore, SeatScript, ServerBots, ServerDir, ServerSetup, SessionDir, SessionOpening,
    SessionTimes, SimServerWorld, UdpLinks, UncheckedRestore, UnixClock,
};
use campfire_net_connection::Connected;
use campfire_net_core::LinkDirection;
use campfire_net_emulator::{DirectionCounts, DuplexModel, LinkModel, Phase};
use campfire_net_link::Unlink;
use campfire_net_payload::UnlinkReason;
use campfire_net_prediction::{PredictionCounts, RollbackMode};
use campfire_net_stepper::{AppId, ClientKind, Init, StepOrder, Stepper, StepperSetup};
use campfire_net_sync::{LocalTimelineSync, TimelineSync};
use campfire_net_udp::ServerIdentity;
use campfire_package::ModePackages;
use campfire_protocol::internals::TestKey;
use campfire_protocol::{Entropy, InputRules, LogReplay, SeedChain, ServerInput, Signer};
use campfire_runner::Session;
use campfire_sim::{EntityIndex, MatchRate, SimTick, SimWorld, StableId};
use campfire_test_packages::{PackageTree, TestMode};
use campfire_units::internals::UnitsWorldInternals;
use campfire_units::{Body, MoveStep, Owner};
use campfire_values::{NameId, Team};
use campfire_visibility::HostVisibility;

use crate::in_process_match::link_layout::{LinkLayout, LinkRows};
use crate::in_process_match::match_stack::{Joiner, MatchStack};
use crate::in_process_match::server_record::ServerRecord;

pub(crate) mod link_layout;
mod match_stack;
pub(crate) mod server_record;

/// Frames a connection gets to link and sync its timeline, and a join to start the match.
const CONNECT_FRAMES: usize = 300;

/// What an `InProcessMatch` runs: how many players join, and how.
#[derive(Debug, Clone)]
pub struct InProcessSetup {
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
    /// How each link carries packets, both ways, on the stack's emulator.
    pub link: LinkModel,
    /// The stepper's seed, from which each link's draws follow.
    pub seed: u64,
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

/// A player's input the server's log applied: its seat, its stamp, and the tick it took effect
/// in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedInput {
    pub seat: Seat,
    pub stamp: Tick,
    pub tick: Tick,
}

/// An app of a match: the server's, or a client's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Server,
    Client(usize),
}

impl InProcessSetup {
    /// The most players a setup holds: the MOBA 3v3's six.
    pub const MAX_PLAYERS: usize = 6;

    /// One player through a perfect link, whose client rolls back only on a misprediction, with
    /// a server that runs a frame a tick.
    pub const SOLO: Self = Self::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN);

    /// A link on which each packet waits 3 to 5 ticks of the lane mode's 30 Hz, 100 to 167 ms,
    /// and none is lost.
    pub const DELAYED_LINK: LinkModel = LinkModel::perfect()
        .with_latency(Duration::from_nanos(133_333_333))
        .with_jitter(Duration::from_nanos(33_333_333));

    /// One player, through perfect links.
    pub const fn solo(rollback: RollbackMode, server_frames: u32, seed_chain: SeedChain) -> Self {
        Self {
            players: 1,
            links: LinkLayout::InOrder,
            rollbacks: [rollback; Self::MAX_PLAYERS],
            server_frames,
            link: LinkModel::perfect(),
            seed: 0,
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
    pub fn duo(link: LinkModel, seed_chain: SeedChain) -> Self {
        Self {
            players: 2,
            link,
            ..Self::solo(RollbackMode::Check, 3, seed_chain)
        }
    }
}

/// A server app and one client app for each player, joined by sealed links on the stack's
/// stepper, each link following the setup's `LinkModel`, each app at its own frame rate on the
/// stepper's one virtual time, so every run repeats exactly (02-engine-core.md, One driver of
/// integrated tests). Each player holds fixed keys and draws fixed random bytes, and the server
/// keeps the state hash after every tick.
#[derive(Debug)]
pub struct InProcessMatch {
    stepper: Stepper<MatchStack>,
    /// How many spectators it added, which takes each one's key.
    spectators: usize,
    setup: InProcessSetup,
    packages: Arc<ModePackages>,
    /// The server's data directory, where it keeps its session, once a test gives it one.
    data: Option<PathBuf>,
    /// The pause and the speed every end follows.
    pace: Arc<Pace>,
    /// The Unix time every end's clock reads, `TestKey::NOW` until a test sets it.
    clock: UnixClock,
    /// What the last step cost each end, kept from step to step.
    step_cost: StepCost,
}

impl InProcessMatch {
    /// The server's session with its seed revealed, as it publishes it: its log's file, and its
    /// hash after each tick it played.
    pub fn server_record(&mut self) -> ServerRecord {
        let world = self.server_mut().world_mut();
        world.resource_mut::<Session>().reveal_seed();
        let mut file = Vec::new();
        world.resource::<Session>().log().encode(&mut file);
        let live = HashTrail::of_totals(world.resource::<TickHashes>().get().iter().copied());
        ServerRecord { file, live }
    }

    /// Asserts that the server's log replays to the hashes it played.
    pub fn assert_log_replays(&mut self) {
        let record = self.server_record();
        record.assert_replays(self.packages());
    }

    /// Steps until `done`, which holds within `steps` steps.
    pub fn step_until(&mut self, steps: usize, done: impl Fn(&Self) -> bool) {
        for _ in 0..steps {
            if done(self) {
                return;
            }
            self.step();
        }
        assert!(done(self), "not done in {steps} steps");
    }

    /// The seed chain the tests' servers commit to.
    pub const SEED_CHAIN: SeedChain = TestKey::seed_chain(NonZeroU32::MIN);

    /// The match scenario's orders by team, the west then the east: each avatar walks 4 m toward
    /// the enemy tower, which kills it there; after it respawns, it walks to a point near the
    /// middle. From their spawns 2 m apart, the two walk on lines that part, so they never touch.
    /// The first order waits for the clients' lead on the server to settle: the stack's sync brings
    /// it to its target by 5 % of a tick a frame.
    pub const SCENARIO_SCRIPTS: [&str; 2] = [
        "[[order]]\ntick = 60\ndirective.orders.move.to = [4, -2]\n[[order]]\ntick = 450\ndirective.orders.move.to = [-3, -2]\n",
        "[[order]]\ntick = 60\ndirective.orders.move.to = [-4, 3]\n[[order]]\ntick = 450\ndirective.orders.move.to = [3, 4]\n",
    ];

    /// A match of the test lane mode at its default rate, its clients connected and synced.
    pub fn new(setup: InProcessSetup) -> Self {
        assert!(
            (1..=2).contains(&setup.players),
            "the lane mode takes 1 or 2 players"
        );
        let packages = PackageTree::of(TestMode::Lane)
            .players_and_saves(&setup.rules, &setup.saves)
            .packages();
        Self::of_mode(setup, packages)
    }

    /// A match of `packages` at their default rate, its clients connected and synced: the
    /// clients run a frame a tick, and the server `setup.server_frames`.
    pub fn of_mode(setup: InProcessSetup, packages: ModePackages) -> Self {
        // The check the stepper holds, started before the packages' load, which logs too.
        let _log = LogCheck::start();
        assert!(
            setup.players <= InProcessSetup::MAX_PLAYERS,
            "a setup holds at most {} players",
            InProcessSetup::MAX_PLAYERS
        );
        assert!(
            setup.server_frames > 0,
            "the server runs a frame a tick at least"
        );
        let packages = Arc::new(packages);
        let pace = Arc::new(Pace::default());
        let clock = UnixClock::manual(TestKey::NOW);
        let rate = Self::rate(&packages);
        let tick = rate.length();
        let server_frame = tick / setup.server_frames;
        assert_eq!(
            server_frame * setup.server_frames,
            tick,
            "the frames make a whole tick"
        );
        let players = setup.players;
        let joiners = (0..players).map(|client| {
            Joiner::Player(match setup.links {
                LinkLayout::InOrder => client,
                LinkLayout::Reversed => players - 1 - client,
            })
        });
        let stack = MatchStack {
            packages: Arc::clone(&packages),
            pace: Arc::clone(&pace),
            clock: clock.clone(),
            rollbacks: setup.rollbacks,
            links: setup.links,
            joiners: joiners.collect(),
        };
        let stepper = Stepper::new(
            stack,
            StepperSetup {
                frame_interval: tick,
                server_frame_interval: server_frame,
                tick_rate: rate.rate(),
                init: Init::Deferred,
                link: DuplexModel::symmetric(setup.link.clone()),
                seed: setup.seed,
                identity: Self::server_identity,
                alpn: UdpLinks::ALPN,
                ..StepperSetup::with_clients(vec![ClientKind::Udp; players])
            },
        );
        let mut local = Self {
            stepper,
            spectators: 0,
            setup,
            packages,
            data: None,
            pace,
            clock,
            step_cost: StepCost::default(),
        };
        local.stepper.connect();
        local.lay_out_links();
        for _ in 0..CONNECT_FRAMES {
            let synced = (0..local.stepper.client_count()).all(|client| {
                let world = local.stepper.client_world(client);
                local.stepper.client(client).contains::<Connected>()
                    && world
                        .get_resource::<LocalTimelineSync>()
                        .is_some_and(TimelineSync::is_synced)
            });
            if synced {
                return local;
            }
            local.step();
        }
        panic!("the clients did not connect and sync in {CONNECT_FRAMES} frames");
    }

    /// The rate of a match of `packages`: their default.
    const fn rate(packages: &ModePackages) -> MatchRate {
        MatchRate::new(packages.manifest().tick_hz.default())
    }

    /// The length of a tick of the match.
    fn tick(&self) -> Duration {
        Self::rate(&self.packages).length()
    }

    /// The signer of the test server's key, from which its TLS key derives.
    fn server_signer() -> Signer {
        Signer::new(TestKey::server(), Entropy::Fixed(5))
    }

    /// The identity of the test server's endpoint, its raw key derived from its own.
    fn server_identity() -> ServerIdentity {
        UdpLinks::identity(&Self::server_signer())
    }

    /// The TLS key every client pins: the test server's raw key.
    fn tls_key() -> TlsKey {
        Self::server_identity()
            .tls_key()
            .expect("a raw key's identity")
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
        self.stepper.stack_mut().joiners.push(joiner);
        let client = self.stepper.add_remote_client();
        self.stepper.connect_client(client);
        self.lay_out_links();
        client
    }

    /// Gives `client` a new sealed link to the server over a new pair of emulated wires, as a
    /// network that works again does: a client that tries its link again connects through it once
    /// its wait passed.
    pub fn mend_link(&mut self, client: usize) {
        self.stepper.relink(client);
        self.lay_out_links();
    }

    /// Gives the server of a reversed layout its clients' addresses, in client order, by which
    /// it puts its links' rows in that order before each of its frames.
    fn lay_out_links(&mut self) {
        let addresses =
            (0..self.stepper.client_count()).map(|client| self.stepper.client_address(client));
        let addresses = addresses.collect();
        if let Some(mut rows) = self
            .stepper
            .server_world_mut()
            .get_resource_mut::<LinkRows>()
        {
            rows.addresses = addresses;
        }
    }

    /// Cuts `client`'s link, as a network that fails does: its wires carry nothing more, and the
    /// server and the client each see it end as a silent peer's.
    pub fn cut_link(&mut self, client: usize) {
        let now = self.stepper.now();
        for direction in [LinkDirection::ClientToServer, LinkDirection::ServerToClient] {
            let wire = self.stepper.client_link(client);
            self.stepper
                .network()
                .add_phase(wire, direction, Phase::outage(now));
        }
        let link = self.link(client);
        self.stepper.server_world_mut().trigger(Unlink {
            entity: link,
            reason: UnlinkReason::TimedOut,
        });
        let entity = self.stepper.client_entity(client);
        self.stepper.client_world_mut(client).trigger(Unlink {
            entity,
            reason: UnlinkReason::TimedOut,
        });
    }

    /// Sets the Unix time every end's clock reads, in seconds.
    pub fn set_clock(&mut self, seconds: u64) {
        self.clock.set(seconds);
    }

    /// Every input the server logged, in the order logged.
    pub fn server_inputs(&self) -> Vec<ServerInput<'_>> {
        let session = self.stepper.server_world().resource::<Session>();
        session.log().server_inputs().collect()
    }

    /// What the server opens or restores its session with: the signer of its key, the setup's
    /// times, a fixed clock and fixed random bytes.
    fn server_setup(&self) -> ServerSetup {
        ServerSetup {
            signer: Arc::new(Self::server_signer()),
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
            .map(|(slot, &text)| {
                SeatScript::new(u32::try_from(slot).expect("a small count"), script(text))
            });
        let takeover = self.setup.takeover.map(script);
        ServerBots::new(slots, takeover.as_ref(), &self.packages)
            .expect("a test's bots name what its mode has")
    }

    /// Gives the server the data directory `dir`, which it holds locked while it runs, where the
    /// session `start_match` opens keeps its directory, private record and journal, and each
    /// client `client-<index>` in it, where it writes its receipts.
    pub fn keep_data(&mut self, dir: PathBuf) {
        assert!(
            !self
                .stepper
                .server_world()
                .contains_resource::<MatchClock>(),
            "a server keeps its data from before the match"
        );
        for client in 0..self.stepper.client_count() {
            let own = ClientDir::open(&dir.join(format!("client-{client}"))).unwrap();
            self.stepper
                .client_world_mut(client)
                .keep_receipts(Arc::new(own));
        }
        let data = ServerDir::open(&dir).unwrap();
        self.stepper.server_world_mut().insert_resource(data);
        self.data = Some(dir);
    }

    /// Ends the server, as a crash after its journal's last sync does; each client sees its
    /// link end as a silent peer's, at once.
    pub fn stop_server(&mut self) {
        self.stepper.stop_server();
        for client in 0..self.stepper.client_count() {
            let entity = self.stepper.client_entity(client);
            self.stepper.client_world_mut(client).trigger(Unlink {
                entity,
                reason: UnlinkReason::TimedOut,
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
        let restore = PendingRestore::check(UncheckedRestore {
            session,
            packages: Arc::clone(&self.packages),
            server: self.server_setup(),
            bots: self.server_bots(),
        })
        .unwrap();
        self.stepper.restart_server(move |world| {
            world.insert_resource(data);
            world.open_session(SessionOpening::Restored(Box::new(restore)));
        });
        self.lay_out_links();
        self.stepper.frame(AppId::Server);
        assert!(
            self.stepper
                .server_world()
                .contains_resource::<MatchClock>(),
            "the session restores in the server's first frame"
        );
    }

    /// `open_match`, then `await_avatars` for `CONNECT_FRAMES`, as the lane mode's avatars come
    /// with its first tick. A client runs match ticks from the match start's message, and its
    /// avatar comes in the replication after the server's first tick, in another packet: which
    /// arrives first varies with how the stack packs and resends them, by the wall clock.
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
            options: Vec::new(),
            seat_picks: Vec::new(),
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
            seed_chain: self.setup.seed_chain.clone(),
            inputs: InputRules::LAN,
            seats: self.setup.players + self.setup.bots.len(),
        })
        .expect("the lane mode runs at its default rate");
        if let Some(data) = self.stepper.server_world().get_resource::<ServerDir>() {
            lobby.keep_files(data).unwrap();
        }
        self.stepper
            .server_world_mut()
            .open_session(SessionOpening::New(Box::new(lobby)));
        for _ in 0..CONNECT_FRAMES {
            let playing = |client: usize| {
                let world = self.stepper.client_world(client);
                world.resource::<JoinState>().playing_clock().is_some()
            };
            if self
                .stepper
                .server_world()
                .contains_resource::<MatchClock>()
                && (0..self.stepper.client_count()).all(playing)
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
        let world = self.stepper.client_world(client);
        self.spawned_avatar(client)
            .is_some_and(|avatar| world.entity_of(avatar).is_some())
    }

    /// Loses every packet the server sends `client` from now on, as a link that drops one way
    /// does, though the client's own packets arrive.
    pub fn lose_server_packets(&mut self, client: usize) {
        let phase = Phase::outage(self.stepper.now());
        self.server_packets(client, phase);
    }

    /// Passes the packets the server sends `client` again.
    pub fn pass_server_packets(&mut self, client: usize) {
        let phase = Phase::recovery(self.stepper.now(), self.setup.link.clone());
        self.server_packets(client, phase);
    }

    /// Makes the packets the server sends `client` follow `phase`.
    fn server_packets(&self, client: usize, phase: Phase) {
        let wire = self.stepper.client_link(client);
        self.stepper
            .network()
            .add_phase(wire, LinkDirection::ServerToClient, phase);
    }

    /// Runs `run` on the server's world in its next fixed tick, before the tick's systems, as the
    /// sim changes its state; the steps that follow run it.
    pub fn server_fixed_update(&mut self, run: impl FnOnce(&mut World) + Send + 'static) {
        self.stepper.server_fixed_update(run);
    }

    /// The server's next frame alone, which shifts where in a tick its frames fall.
    pub fn server_frame(&mut self) {
        self.stepper.frame(AppId::Server);
    }

    /// One tick of every end, each at its own frame rate: a frame of each client, and the
    /// server's frames, which fall together with the clients' at the tick's end run after them;
    /// what each end's frames cost kept in `step_cost`. A server whose session went back to a
    /// save then starts again on its data, as a local server does.
    pub fn step(&mut self) {
        let apps = (0..self.stepper.client_count()).map(AppId::Client);
        for app in apps.chain([AppId::Server]) {
            self.stepper.take_slowest_frame(app);
        }
        self.stepper.run_for(self.tick(), StepOrder::ClientsFirst);
        self.step_cost.clear();
        for client in 0..self.stepper.client_count() {
            let spent = self.stepper.take_slowest_frame(AppId::Client(client));
            self.step_cost.clients.push(spent);
        }
        self.step_cost.server = self.stepper.take_slowest_frame(AppId::Server);
        if self.stepper.server_world().is_reload_wanted() {
            self.restart_server();
        }
    }

    /// `ticks` ticks of the clients alone, then a frame of the server as long as their ticks: a
    /// server whose process stalled, and whose clients ran on and sent their inputs meanwhile.
    pub fn stall_server(&mut self, ticks: u32) {
        let stall = self.tick() * ticks;
        self.stepper.stall(AppId::Server, stall);
        self.stepper.run_for(stall, StepOrder::ClientsFirst);
    }

    /// A stall of the whole host for `ticks` ticks, as an OS that stops every process: the
    /// clients run no frame while the wire clock runs on, and then each takes the stalled time in
    /// one frame, which its virtual clock cuts to its `max_delta`. The server, which its wake
    /// brought back during the stall, runs a frame each `max_delta` of it, so it runs every tick
    /// it missed.
    pub fn stall_host(&mut self, ticks: u32) {
        let stall = self.tick() * ticks;
        let clients = self.stepper.client_count();
        for client in 0..clients {
            self.stepper.stall(AppId::Client(client), stall);
        }
        let world = self.stepper.server_world();
        let most = world.resource::<Time<Virtual>>().max_delta();
        let mut left = stall;
        while left > Duration::ZERO {
            let frame = left.min(most);
            self.stepper.advance_time(frame);
            self.stepper.frame(AppId::Server);
            left -= frame;
        }
        for client in 0..clients {
            self.stepper.frame(AppId::Client(client));
        }
    }

    /// Each input of a player that the server's log applied, in the order of its ticks, from the
    /// log's latest checkpoint on.
    pub fn applied_inputs(&self) -> Vec<AppliedInput> {
        let log = self.stepper.server_world().resource::<Session>().log();
        let mut inputs = Vec::new();
        let Some(mut replay) = LogReplay::of(log) else {
            return inputs;
        };
        while !replay.is_done(log) {
            let tick = replay.next_tick();
            inputs.extend(replay.seal_tick(log).map(|input| AppliedInput {
                seat: input.seat,
                stamp: input.stamp,
                tick,
            }));
        }
        inputs
    }

    /// The next frame of `client` alone: one tick of it.
    pub fn client_frame(&mut self, client: usize) {
        self.stepper.frame(AppId::Client(client));
    }

    pub const fn setup(&self) -> &InProcessSetup {
        &self.setup
    }

    /// Gives `client`'s avatar an order, sent in the client's next tick.
    pub fn order(&mut self, client: usize, directive: impl Into<Directive>) {
        let unit = self.avatar(client);
        self.stepper
            .client_world_mut(client)
            .resource_mut::<PendingOrders>()
            .push(Order::one(unit, directive));
    }

    /// Makes `client` play `script`, as a bot does.
    pub fn play(&mut self, client: usize, script: &OrderScript) {
        let books = self.packages.name_books();
        let bot = BotScript::new(script, &BotNames::new(&books))
            .expect("a test's bot names what its mode has");
        self.stepper.client_world_mut(client).insert_resource(bot);
    }

    /// `client`'s player's avatar, as the server holds it.
    pub fn avatar(&self, client: usize) -> StableId {
        self.spawned_avatar(client)
            .expect("the match started, with the player's avatar")
    }

    /// `client`'s player's avatar, as the server holds it, once the server spawned it.
    fn spawned_avatar(&self, client: usize) -> Option<StableId> {
        let world = self.stepper.server_world();
        let slot = world
            .link_seat(self.link(client))
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
        let world = self.stepper.server_world();
        let avatar = world.entity_of(self.avatar(client));
        *world
            .get::<Team>(avatar.expect("the avatar exists"))
            .expect("an avatar has a team")
    }

    pub const fn server(&self) -> &App {
        self.stepper.server_app()
    }

    pub fn app(&self, end: End) -> &App {
        match end {
            End::Server => self.stepper.server_app(),
            End::Client(client) => self.stepper.client_app(client),
        }
    }

    /// The sim tick `end` runs next.
    pub fn next_tick(&self, end: End) -> u64 {
        self.app(end).world().resource::<SimTick>().start().get()
    }

    /// The times `client` rolled its state back.
    pub fn rollbacks(&self, client: usize) -> u32 {
        let world = self.stepper.client_world(client);
        world.resource::<PredictionCounts>().rollbacks
    }

    /// The client whose player's avatar is on `team`.
    pub fn client_of(&self, team: Team) -> usize {
        (0..self.stepper.client_count())
            .find(|&client| self.team(client) == team)
            .expect("a client plays each team")
    }

    /// The tower of `team` on the server: its one unit that stands, has a body and no owner.
    pub fn tower(&self, team: Team) -> StableId {
        let world = self.stepper.server_world();
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
        self.stepper.server_app_mut()
    }

    pub fn client(&self, client: usize) -> &App {
        self.stepper.client_app(client)
    }

    pub fn client_mut(&mut self, client: usize) -> &mut App {
        self.stepper.client_app_mut(client)
    }

    /// Makes the server give its receipts in its next update, as if their second passed.
    pub fn give_receipts(&mut self) {
        self.stepper.server_world_mut().give_receipts();
    }

    /// Holds the server's next round of receipts until `give_receipts`, so a test that steps
    /// slower than real time, as one does under a loaded host, meets none it did not ask for.
    pub fn hold_receipts(&mut self) {
        self.stepper.server_world_mut().hold_receipts();
    }

    /// The server's link to `client`, as the stepper finds it, or a placeholder before the
    /// server's endpoint spawned one.
    pub fn link(&self, client: usize) -> Entity {
        let link = self.stepper.try_client_of_entity(client);
        link.unwrap_or(Entity::PLACEHOLDER)
    }

    /// What `direction` of `client`'s wire did with its packets so far.
    pub fn wire_counts(&self, client: usize, direction: LinkDirection) -> DirectionCounts {
        let wire = self.stepper.client_link(client);
        self.stepper.network().counts(wire, direction)
    }

    /// The pause and the speed every end follows.
    pub fn pace(&self) -> &Pace {
        &self.pace
    }

    /// The events at Warn and Error that the match logged and no test took yet.
    pub const fn log(&self) -> &LogCheck {
        self.stepper.log()
    }

    /// The packages of the session's mode.
    pub fn packages(&self) -> &ModePackages {
        &self.packages
    }
}

pub(crate) mod bench {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use std::{env, process};

    use campfire_bots::OrderScript;
    use campfire_math::{Num, Vec3};
    use campfire_net_emulator::LinkModel;
    use campfire_net_prediction::RollbackMode;
    use campfire_orders::OrdersDirective;
    use campfire_sim::{Position, SimWorld};
    use campfire_store::DurableDir;
    use campfire_test_packages::TestMode;
    use campfire_units::GroundPoint;

    use crate::in_process_match::{CONNECT_FRAMES, End, InProcessMatch, InProcessSetup, StepCost};

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
            let mut local = Self::new(InProcessSetup::solo(rollback, 1, Self::SEED_CHAIN));
            local.start_match();
            local
        }

        /// The MOBA 3v3 at its default rate, a server frame a tick, with a player in each
        /// of the first slots, whose clients roll back as `ROLLBACKS_3V3` says, and the
        /// server's bots in the others, every slot picking as `PICKS_3V3` says; played to its
        /// first wave, each client holding its hero, for `walk_steps`.
        pub(crate) fn walking_3v3() -> Self {
            let players = ROLLBACKS_3V3.len();
            let mut setup = InProcessSetup::solo(RollbackMode::Check, 1, Self::SEED_CHAIN);
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
            let world = self.stepper.server_world();
            for client in 0..self.stepper.client_count() {
                let avatar = world.entity_of(self.avatar(client));
                let position = world.get::<Position>(avatar.expect("the avatar exists"));
                places.push(position.expect("an avatar has a place").get());
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
            debug_assert_eq!(around.len(), self.stepper.client_count());
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
                        self.order(
                            client,
                            OrdersDirective::Move {
                                to: GroundPoint { x, z },
                            },
                        );
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
            let mut local = Self::new(InProcessSetup::duo(LinkModel::perfect(), Self::SEED_CHAIN));
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
