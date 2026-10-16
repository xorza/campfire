use std::time::Duration;

use bevy_app::{
    App, FixedUpdate, Last, Plugin, PostUpdate, RunFixedMainLoop, RunFixedMainLoopSystems, Update,
};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Added, QueryState};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::common_conditions::resource_exists;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use bevy_time::{Real, Time, Virtual};
use campfire_combat::Deaths;
use campfire_common::{PlayerSlot, Tick};
use campfire_declare::CapabilitySet;
use campfire_log::{ErrorReport, LogEvent};
use campfire_mode::{MatchEnd, ModeWorld};
use campfire_package::ModePackages;
use campfire_protocol::{
    Applied, Controller, ServerInput, ServerSeeds, SessionLog, SessionTerms, Signer,
};
use campfire_quic::{CloseLink, QuicPlugin};
use campfire_runner::{Session, SessionRules, SessionSource, SessionWorld, StartError, TermsError};
use campfire_sim::{SimTick, SimWorld, TickRate};
use lightyear::core::tick::TickDuration;
use lightyear::prelude::server::ServerPlugins;
use lightyear::prelude::{
    LinkOf, LinkSystems, LocalTimeline, MessageReceiver, MessageSender, ReplicationSender,
    RoomAllocator, RoomPlugin, Tick as NetTick,
};
use tracing::{trace, trace_span};

use crate::close_code::CloseCode;
use crate::events::input_logged::InputLogged;
use crate::events::input_message_refused::InputMessageRefused;
use crate::events::input_message_unfit::InputMessageUnfit;
use crate::events::input_never_applied::{InputNeverApplied, Unapplied};
use crate::events::match_ended::MatchEnded;
use crate::events::save_refused::SaveRefused;
use crate::events::server_frame::ServerFrame;
use crate::events::ticks_caught_up::TicksCaughtUp;
use crate::events::time_dropped::TimeDropped;
use crate::events::unit_died::UnitDied;
use crate::faults::Faults;
use crate::input_ack::InputAck;
use crate::input_message::InputMessage;
use crate::match_clock::MatchClock;
use crate::match_start::MatchStart;
use crate::net_protocol::{MatchChannel, NetProtocol};
use crate::pace::PaceSpeed;
use crate::save_command::SaveCommand;
use crate::sim_server::bot_driver::BotDriver;
use crate::sim_server::checkpoints::Checkpoints;
use crate::sim_server::data_filter::DataFilters;
use crate::sim_server::door::Door;
use crate::sim_server::error::RestoreMatchError;
use crate::sim_server::journal_watch::JournalWatch;
use crate::sim_server::kind_shows::{KindShows, Shows};
use crate::sim_server::lobby::Lobby;
use crate::sim_server::match_visibility::MatchVisibility;
use crate::sim_server::offering::{Dismissing, Offered, Offering};
use crate::sim_server::pending_restore::PendingRestore;
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::receipts::Receipts;
use crate::sim_server::seats::Seats;
use crate::sim_server::server_bots::ServerBots;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::session_dir::{RestoredSession, SessionFiles};
use crate::sim_server::session_journal::SessionJournal;
use crate::sim_server::sim_server_world::{SimServerWorld, SimServerWorldCrate};
use crate::sim_server::snapshot_dir::SnapshotDir;
use crate::sim_server::spectators::Spectators;
use crate::sim_server::team_rooms::TeamRooms;
use crate::sim_server::tick_hashes::TickHashes;
use crate::sim_server::viewer::Viewer;

pub(crate) mod bot_driver;
pub(crate) mod checkpoint_thread;
pub(crate) mod checkpoints;
pub(crate) mod data_filter;
pub(crate) mod door;
pub(crate) mod error;
pub(crate) mod journal_watch;
pub(crate) mod key_file;
pub(crate) mod kind_shows;
pub(crate) mod lobby;
pub(crate) mod local_session;
pub(crate) mod match_visibility;
pub(crate) mod offering;
pub(crate) mod pending_restore;
pub(crate) mod player_link;
pub(crate) mod receipts;
pub(crate) mod seats;
pub(crate) mod server_bots;
pub(crate) mod server_dir;
pub(crate) mod server_layout;
pub(crate) mod server_setup;
pub(crate) mod session_dir;
pub(crate) mod session_journal;
pub(crate) mod sim_server_plugins;
pub(crate) mod sim_server_world;
pub(crate) mod slot_bot_file;
pub(crate) mod snapshot_dir;
pub(crate) mod spectators;
pub(crate) mod team_rooms;
pub(crate) mod tick_hashes;
pub(crate) mod viewer;

/// Runs a session on a Lightyear server: while a `Lobby` is open, lets players join, or restores
/// a `PendingRestore`; then records the packets players send, runs one sim tick in each fixed
/// tick, and sends each client the units its team sees. It hashes the state after a tick only
/// while the world holds `TickHashes`. It adds Lightyear's server at the tick's length `tick`, the
/// protocol, and the replication to each new link; `SimServerPlugins` adds it.
#[derive(Debug)]
pub(crate) struct SimServer {
    pub(crate) tick: Duration,
    /// The mode's declared capabilities, whose components alone replicate.
    pub(crate) capabilities: CapabilitySet,
}

impl Plugin for SimServer {
    fn build(&self, app: &mut App) {
        app.add_plugins(ServerPlugins {
            tick_duration: self.tick,
        });
        app.add_plugins((
            NetProtocol {
                capabilities: self.capabilities,
            },
            RoomPlugin,
            QuicPlugin,
        ));
        DataFilters::register(app, self.capabilities);
        KindShows::register(app);
        let rooms = TeamRooms::new(&mut app.world_mut().resource_mut::<RoomAllocator>());
        app.insert_resource(rooms);
        app.add_observer(
            |added: On<'_, '_, Add, LinkOf>, mut commands: Commands<'_, '_>| {
                commands.entity(added.entity).insert(ReplicationSender);
            },
        );
        app.init_resource::<Faults>();
        app.add_systems(
            Update,
            PendingRestore::run.run_if(resource_exists::<PendingRestore>),
        );
        app.add_systems(
            Update,
            (
                Lobby::free_seats,
                Offering::offer_links::<Lobby>,
                Lobby::take_joins,
                Lobby::start_when_full,
            )
                .chain()
                .run_if(resource_exists::<Lobby>),
        );
        // Each system that logs checks that the session runs as it starts, as another system of
        // the frame may end it.
        app.add_systems(
            Update,
            (Offering::offer_links::<Door>, Door::take_joins, Door::watch)
                .chain()
                .distributive_run_if(resource_exists::<Door>)
                .distributive_run_if(session_running),
        );
        app.add_systems(
            Update,
            Receipts::give
                .after(Door::watch)
                .run_if(resource_exists::<Receipts>)
                .run_if(session_running),
        );
        app.add_systems(
            Update,
            Checkpoints::finish.run_if(resource_exists::<Checkpoints>),
        );
        app.add_systems(
            Update,
            JournalWatch::warn_slow.run_if(resource_exists::<JournalWatch>),
        );
        app.add_systems(
            Update,
            Self::take_save_commands
                .after(Door::watch)
                .run_if(resource_exists::<Door>)
                .run_if(session_running),
        );
        app.add_systems(Update, Spectators::choose_teams);
        app.add_observer(Spectators::leave);
        Viewer::follow_links(app);
        Viewer::follow_out_of_play(app);
        app.add_systems(Last, Dismissing::tell);
        app.add_systems(
            PostUpdate,
            (Dismissing::end, Offered::close_silent).after(LinkSystems::Send),
        );
        // Lightyear keeps a received message for one frame only, and a frame runs no fixed tick
        // or several, so the inputs are logged in every frame, before its fixed ticks; a frame
        // that runs several is logged, as an input that missed its start waits for all of them.
        app.add_systems(
            RunFixedMainLoop,
            (
                record_inputs.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
                report_catch_up.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop),
                report_dropped_time.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
            )
                .distributive_run_if(resource_exists::<MatchClock>)
                .distributive_run_if(session_running),
        );
        app.add_systems(
            FixedUpdate,
            (
                BotDriver::drive,
                run_sim_tick,
                record_hash.run_if(resource_exists::<TickHashes>),
                // Right after the tick, before any entry of the next.
                Self::begin_checkpoint.run_if(resource_exists::<Checkpoints>),
                report_deaths,
                report_end,
            )
                .chain()
                .in_set(SimTickSystems),
        );
        app.configure_sets(FixedUpdate, SimTickSystems.run_if(sim_tick_due));
        app.configure_sets(
            FixedUpdate,
            Shows
                .in_set(SimTickSystems)
                .after(Self::begin_checkpoint)
                .before(report_deaths),
        );
    }
}

/// The systems of a fixed tick that runs a sim tick, whose condition is checked once, before the
/// first of them, so the tick runs whole even when it ends the session.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SimTickSystems;

/// The session went back to a save: its server starts again on its data, which restores the
/// match from it.
#[derive(Resource, Debug)]
struct Reload;

/// What a match starts from: the log of its header, the seeds of its segments, its mode's
/// packages, its journal and snapshots' directory if the server keeps them, and the server's
/// setup.
#[derive(Debug)]
pub(crate) struct SessionStart<'a> {
    pub(crate) log: SessionLog,
    pub(crate) seeds: ServerSeeds,
    pub(crate) packages: &'a ModePackages,
    pub(crate) files: Option<SessionFiles>,
    pub(crate) server: &'a ServerSetup,
    pub(crate) bots: ServerBots,
    /// The lobby's offering, which the door offers from then on.
    pub(crate) offering: Offering,
}

/// What a match runs with from the next fixed tick, as it starts or restores: the offering its
/// door offers, the journal and the snapshots' directory when the server keeps them, its seats,
/// and the server's bots.
#[derive(Debug)]
struct MatchRun {
    offering: Offering,
    journal: Option<SessionJournal>,
    snapshots: Option<SnapshotDir>,
    seats: Seats,
    bots: ServerBots,
}

/// The save commands' links, each with its player.
type SaveLinks = QueryState<(
    &'static PlayerLink,
    &'static mut MessageReceiver<SaveCommand>,
)>;

impl SimServer {
    /// Begins the checkpoint due at the boundary before the next tick, as `Checkpoints::begin`
    /// does, and ends the session with the outcome it gives.
    fn begin_checkpoint(world: &mut World) {
        if let Some(outcome) = Checkpoints::begin(world) {
            world
                .end_session(outcome)
                .expect("no checkpoint is on the thread");
        }
    }

    /// Takes the save commands of the seated players, as `Checkpoints::take` does, and loads the
    /// save a load names; each refused one is logged.
    fn take_save_commands(
        world: &mut World,
        (mut links, mut taken): (Local<'_, SaveLinks>, Local<'_, Vec<SaveCommand>>),
    ) {
        for (link, mut receiver) in links.iter_mut(world) {
            if !link.refused() {
                taken.extend(receiver.receive());
            }
        }
        for command in taken.drain(..) {
            match Checkpoints::take(world, command) {
                Ok(None) => {}
                Ok(Some(segment)) => world
                    .load_session_save(segment)
                    .expect("no checkpoint is on the thread"),
                Err(refusal) => SaveRefused { refusal }.log(),
            }
        }
    }

    /// Starts the match of `start.log`'s header in the next fixed tick, recording into its log,
    /// each segment drawing from its seed. From then on a frame advances the server's clock by
    /// the ticks of the max input delay less one at most, a tick at least, so a burst after a
    /// stall makes no on-time input late; a longer frame's time past that is dropped, and logged.
    /// `clients` are the links of the players, each with its slot; each learns its slot and the
    /// start tick. The server's bots play their slots.
    /// From the first tick on, every unit replicates to the clients whose team sees it, and the
    /// owner's client predicts it, but a projectile or an area. With its files, the log goes into
    /// the journal as the server logs it, `JournalWatch` watches it, and the server takes the
    /// checkpoints due. The door takes the joins from then on.
    fn start_match(
        world: &mut World,
        start: SessionStart<'_>,
        clients: &[(PlayerSlot, Entity)],
    ) -> Result<(), StartError> {
        let SessionStart {
            log,
            seeds,
            packages,
            files,
            server,
            bots,
            offering,
        } = start;
        assert_eq!(
            clients.len(),
            log.header().players().count(),
            "one client per player"
        );
        let terms = log.header().terms.clone();
        Self::take_terms(world, &terms, packages).map_err(StartError::Terms)?;
        let slots = log.slot_count();
        Session::start(
            world,
            SessionSource {
                log,
                seeds,
                packages,
                signer: Some(Signer::new(server.key, server.os.entropy)),
            },
        )?;
        let (journal, snapshots) = files.map(|files| (files.journal, files.snapshots)).unzip();
        let run = MatchRun {
            offering,
            journal,
            snapshots,
            seats: Seats::new(slots, clients),
            bots,
        };
        let start = Self::run_match(world, run);
        let next = world.resource::<Session>().log().next_tick();
        for &(slot, client) in clients {
            let team = world.team_of(slot).expect("every player has a team");
            world.entity_mut(client).insert(PlayerLink::new(slot, team));
            world
                .get_mut::<MessageSender<MatchStart>>(client)
                .expect("a client link sends the match start")
                .send::<MatchChannel>(MatchStart {
                    start_tick: start.0,
                    first: next,
                    slot,
                    team,
                    chain: None,
                    loaded: false,
                });
        }
        Ok(())
    }

    /// Takes the session's `terms` before its match starts or restores: bounds the server's
    /// frames, so a frame advances its clock by the ticks of the max input delay less one at
    /// most, a tick at least, each as long as the clock's tick now, which a local match's pace may
    /// have set; and keeps their visibility, which each data filter takes its audience from as
    /// the match's entities spawn. An error for terms the rules of `packages` do not admit, which
    /// the session's start refuses as well, after the match's entities spawned.
    fn take_terms(
        world: &mut World,
        terms: &SessionTerms,
        packages: &ModePackages,
    ) -> Result<(), TermsError> {
        let session = TickRate::new(terms.tick_hz).length();
        let tick = world.resource::<TickDuration>().0;
        assert!(
            PaceSpeed::ALL
                .iter()
                .any(|speed| speed.tick(session) == tick),
            "the server ticks at the session's rate at a speed, not every {tick:?}"
        );
        let burst = terms.inputs.max_delay.get().saturating_sub(1).max(1);
        let burst = u32::try_from(burst).expect("a max input delay of a LAN session fits u32");
        world
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(tick * burst);
        let visibility = SessionRules::of(packages).check(terms)?;
        world
            .resource_mut::<TeamRooms>()
            .set_spectators(&visibility);
        world.insert_resource(MatchVisibility(visibility));
        Ok(())
    }

    /// Runs the match of the world's session from the next fixed tick, as it starts or restores:
    /// the door offers `run.offering`, the log goes into the journal, which `JournalWatch`
    /// watches, and the checkpoints into the snapshots' directory when the server keeps them, and
    /// the server's bots play their slots. Gives the Lightyear tick the session's next tick runs
    /// in.
    fn run_match(world: &mut World, run: MatchRun) -> NetTick {
        let MatchRun {
            offering,
            journal,
            snapshots,
            seats,
            bots,
        } = run;
        if let Some(journal) = journal {
            world.insert_resource(JournalWatch(journal.watch()));
            world
                .resource_mut::<Session>()
                .keep_journal(Box::new(journal));
        }
        world.insert_resource(Door::new(offering));
        if let Some(snapshots) = snapshots {
            Checkpoints::start(world, snapshots);
        }
        let log = world.resource::<Session>().log();
        let (slots, next) = (log.slot_count(), log.next_tick());
        world.insert_resource(seats);
        world.insert_resource(Receipts::new(slots));
        let start = world.resource::<LocalTimeline>().tick() + 1;
        world.insert_resource(MatchClock::new(start, next));
        world.insert_resource(FrameStart(next));
        let driver = BotDriver::new(bots, next, world);
        world.insert_resource(driver);
        start
    }

    /// Restores the match of `restored`, a session a crash or a failed journal ended, of the mode
    /// `packages` holds, on the server of `server`: builds it from the snapshot of the latest
    /// checkpoint whose record the log holds, or from tick 0 with none, and replays its log from
    /// there to the tick the server stopped before, keeping each tick's state hash while the
    /// world holds `TickHashes`; a checkpoint begun with no record is taken again as the replay
    /// reaches its boundary, and one due at the last boundary, as a save the mode asked for,
    /// begins there. It logs `Disconnected` for each slot a player controls, whose grace
    /// period runs from then, and runs the match on from the next fixed tick, so the ticks the
    /// stop lost take no time in the sim. The journal goes on from its last record, and
    /// `JournalWatch` watches it. No client is linked: the players come back through the door.
    pub(crate) fn restore_match(
        world: &mut World,
        restored: RestoredSession,
        packages: &ModePackages,
        server: &ServerSetup,
        bots: ServerBots,
    ) -> Result<(), RestoreMatchError> {
        let terms = restored.log.header().terms.clone();
        Self::take_terms(world, &terms, packages)
            .map_err(|error| RestoreMatchError::Start(StartError::Terms(error)))?;
        let slots = restored.log.slot_count();
        let signer = Signer::new(server.key, server.os.entropy);
        let snapshots = restored.replay(world, packages, signer, |world| {
            if world.contains_resource::<TickHashes>() {
                record_hash(world);
            }
        })?;
        let players: Vec<PlayerSlot> = world.resource::<Session>().log().player_slots().collect();
        let now = world.resource::<Time<Real>>().elapsed();
        let run = MatchRun {
            offering: Offering::new(terms, server),
            journal: None,
            snapshots: Some(snapshots),
            seats: Seats::gone(slots, players.iter().copied(), now),
            bots,
        };
        Self::run_match(world, run);
        // The stop may have come after the tick before this boundary, before the save it made due
        // here began.
        Self::begin_checkpoint(world);
        let mut session = world.resource_mut::<Session>();
        for &slot in &players {
            session
                .serve(ServerInput::Disconnected { slot })
                .expect("the server's own input holds");
        }
        Ok(())
    }
}

/// Logs each packet received in this frame, before the next tick runs, and tells its player where
/// each of its inputs takes effect. A refused packet ends its link: a client that follows the
/// rules sends none, and its chain no longer matches the log's. The links go in the order of
/// their slots, as the log's order of one frame's inputs must not follow the query's.
fn record_inputs(
    mut commands: Commands<'_, '_>,
    mut links: Query<
        '_,
        '_,
        (
            Entity,
            &PlayerLink,
            &mut MessageReceiver<InputMessage>,
            &mut MessageSender<InputAck>,
        ),
    >,
    mut session: ResMut<'_, Session>,
    mut frame: ResMut<'_, FrameStart>,
    mut applied: Local<'_, Vec<Applied>>,
    mut order: Local<'_, Vec<(PlayerSlot, Entity)>>,
) {
    frame.0 = session.log().next_tick();
    ServerFrame { tick: frame.0 }.log();
    order.extend(links.iter().map(|(entity, link, ..)| (link.slot(), entity)));
    order.sort_unstable();
    for (_, entity) in order.drain(..) {
        let (entity, &(mut link), mut receiver, mut acks) =
            links.get_mut(entity).expect("a link the query just gave");
        for message in receiver.receive() {
            if link.refused() {
                continue;
            }
            let next_tick = session.log().next_tick();
            let slot = link.slot();
            let Some(inputs) = message.inputs(slot) else {
                InputMessageUnfit { slot }.log();
                link = link.into_refused();
                commands.entity(entity).insert(link);
                commands.trigger(CloseLink {
                    entity,
                    code: CloseCode::InputRefused.app(),
                });
                continue;
            };
            if let Err(error) = session.record(inputs.clone(), message.signature(), &mut applied) {
                InputMessageRefused {
                    slot: link.slot(),
                    next_tick,
                    error: ErrorReport::of(&error).to_string(),
                }
                .log();
                link = link.into_refused();
                commands.entity(entity).insert(link);
                commands.trigger(CloseLink {
                    entity,
                    code: CloseCode::InputRefused.app(),
                });
                continue;
            }
            let Some(Controller::Player { chain, .. }) = session.log().controller(link.slot())
            else {
                unreachable!("a packet the log took is of a slot a player controls");
            };
            let first_seq = chain.next_seq() - u64::try_from(applied.len()).expect("a count fits");
            acks.send::<MatchChannel>(InputAck::of(first_seq, &applied));
            for ((input, &outcome), seq) in inputs.zip(applied.iter()).zip(first_seq..) {
                let outcome = match outcome {
                    Applied::At(tick) => {
                        InputLogged {
                            slot: link.slot(),
                            seq,
                            stamp: input.stamp,
                            tick,
                        }
                        .log();
                        continue;
                    }
                    Applied::Late => Unapplied::Late,
                    Applied::Early => Unapplied::Early,
                };
                InputNeverApplied {
                    slot: link.slot(),
                    stamp: input.stamp,
                    next_tick,
                    outcome,
                }
                .log();
            }
        }
    }
}

/// The next tick as the frame's fixed ticks start, which `record_inputs` keeps for
/// `report_catch_up`; the match's start gives it its first.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
struct FrameStart(Tick);

/// Logs the ticks of a frame that ran more than one, from the next tick as it started.
fn report_catch_up(session: Res<'_, Session>, start: Res<'_, FrameStart>) {
    if let Some(caught_up) = TicksCaughtUp::of(start.0, session.log().next_tick()) {
        caught_up.log();
    }
}

/// Logs the time this frame dropped past the most a frame may advance.
fn report_dropped_time(real: Res<'_, Time<Real>>, virtual_time: Res<'_, Time<Virtual>>) {
    if let Some(dropped) = TimeDropped::of(real.delta(), virtual_time.max_delta()) {
        dropped.log();
    }
}

/// The match has started and its session has not ended, so this fixed tick runs a sim tick.
fn sim_tick_due(
    timeline: Res<'_, LocalTimeline>,
    clock: Option<Res<'_, MatchClock>>,
    session: Option<Res<'_, Session>>,
    reload: Option<Res<'_, Reload>>,
) -> bool {
    clock.is_some_and(|clock| clock.sim_tick(timeline.tick()).is_some())
        && session_running(session, reload)
}

/// The session has started, has not ended, and has not gone back to a save.
fn session_running(session: Option<Res<'_, Session>>, reload: Option<Res<'_, Reload>>) -> bool {
    reload.is_none() && session.is_some_and(|session| session.log().result().is_none())
}

fn run_sim_tick(world: &mut World) {
    let tick = world.resource::<LocalTimeline>().tick();
    let sim_tick = world.resource::<SimTick>().start();
    debug_assert_eq!(
        world.resource::<MatchClock>().sim_tick(tick),
        Some(sim_tick),
        "the server runs every sim tick once, in order"
    );
    let _tick = trace_span!("tick", n = sim_tick.get()).entered();
    world.run_session_tick();
}

fn record_hash(world: &mut World) {
    let hash = world.state_hash();
    let mut hashes = world.resource_mut::<TickHashes>();
    trace!(tick = hashes.get().len(), %hash, "hashed the state");
    hashes.push(hash);
}

/// Logs each unit that died in the tick just run, from combat's record of the tick's deaths,
/// which still names a unit that despawned as it died. A match that ended runs no damage, and its
/// record holds an older tick, which is logged already.
fn report_deaths(tick: Res<'_, SimTick>, deaths: Option<Res<'_, Deaths>>) {
    let Some(deaths) = deaths else {
        return;
    };
    let ran = Tick::new(tick.start().get() - 1);
    if deaths.tick() != ran {
        return;
    }
    for death in deaths.iter() {
        UnitDied::of(ran, &death).log();
    }
}

/// Logs the end of the match the tick just run ended, which reaches the clients on the match's
/// entity.
fn report_end(ended: Query<'_, '_, &MatchEnd, Added<MatchEnd>>) {
    for &end in &ended {
        MatchEnded::of(end).log();
    }
}
