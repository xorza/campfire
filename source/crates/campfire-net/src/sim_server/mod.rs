use std::time::Duration;

use bevy_app::{
    App, FixedUpdate, Last, Plugin, PostUpdate, RunFixedMainLoop, RunFixedMainLoopSystems, Update,
};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::Added;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::common_conditions::resource_exists;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::Mut;
use bevy_ecs::world::World;
use bevy_time::{Real, Time, Virtual};
use campfire_capabilities::{CapabilitySet, Deaths, MatchEnd, MatchResult, ModeWorld};
use campfire_common::{PlayerSlot, Tick};
use campfire_log::{ErrorReport, LogEvent};
use campfire_package::ModePackages;
use campfire_protocol::ServerInput;
use campfire_protocol::{Applied, Controller, ServerSeeds, SessionLog, SessionTerms};
use campfire_quic::{CloseLink, QuicPlugin};
use campfire_runner::{Session, SessionRules, SessionWorld, StartError, TermsError};
use campfire_sim::StateRegistry;
use campfire_sim::{SimTick, SimWorld, TickRate};
use campfire_store::InputFile;
use lightyear::core::tick::TickDuration;
use lightyear::prelude::server::ServerPlugins;
use lightyear::prelude::{
    LinkOf, LinkSystems, LocalTimeline, MessageReceiver, MessageSender, ReplicationSender,
    RoomAllocator, RoomPlugin,
};
use tracing::{info, trace, trace_span};

use crate::close_code::CloseCode;
use crate::events::input_logged::InputLogged;
use crate::events::input_message_refused::InputMessageRefused;
use crate::events::input_message_unfit::InputMessageUnfit;
use crate::events::input_never_applied::{InputNeverApplied, Unapplied};
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
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::receipts::Receipts;
use crate::sim_server::seats::Seats;
use crate::sim_server::server_bots::ServerBots;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::server_signer::ServerSigner;
use crate::sim_server::session_dir::{RestoredSession, SessionFiles};
use crate::sim_server::session_journal::SessionJournal;
use crate::sim_server::spectators::Spectators;
use crate::sim_server::team_rooms::TeamRooms;
use crate::sim_server::tick_hashes::TickHashes;
use crate::sim_server::viewer::Viewer;

#[cfg(feature = "internals")]
pub(crate) mod bench;
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
pub(crate) mod match_visibility;
pub(crate) mod offering;
pub(crate) mod player_link;
pub(crate) mod receipts;
pub(crate) mod scope_union;
pub(crate) mod seats;
pub(crate) mod server_bots;
pub(crate) mod server_dir;
pub(crate) mod server_layout;
pub(crate) mod server_setup;
pub(crate) mod server_signer;
pub(crate) mod session_dir;
pub(crate) mod session_journal;
pub(crate) mod sim_server_world;
pub(crate) mod slot_bot_file;
pub(crate) mod spectators;
pub(crate) mod team_rooms;
pub(crate) mod tick_hashes;
pub(crate) mod viewer;

/// Runs a session on a Lightyear server: while a `Lobby` is open, lets players join; then records
/// the packets players send, runs one sim tick in each fixed tick, and sends each client the units
/// its team sees. It hashes the state after a tick only while the world holds `TickHashes`. It
/// adds Lightyear's server at the tick's length `tick`, the protocol, and the replication to each
/// new link; its app adds its frame loop or clock, its session, and its transport: QUIC's, or
/// in-process channels.
#[derive(Debug)]
pub struct SimServer {
    pub tick: Duration,
    /// The mode's declared capabilities, whose components alone replicate.
    pub capabilities: CapabilitySet,
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
            (
                Lobby::free_seats,
                Lobby::offer,
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
            (Door::offer, Door::take_joins, Door::watch)
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
            Checkpoints::take_commands
                .after(Door::watch)
                .run_if(resource_exists::<Door>)
                .run_if(session_running),
        );
        app.add_systems(Update, Spectators::choose_teams);
        app.add_observer(Spectators::leave);
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
                Checkpoints::begin.run_if(resource_exists::<Checkpoints>),
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
                .after(Checkpoints::begin)
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
}

impl SimServer {
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
        } = start;
        assert_eq!(
            clients.len(),
            log.header().players().count(),
            "one client per player"
        );
        let terms = log.header().terms.clone();
        SimServer::bound_frames(world, &terms);
        SimServer::set_visibility(world, &terms).map_err(StartError::Terms)?;
        let slots = log.slot_count();
        Session::start(world, log, seeds, packages)?;
        let (journal, snapshots) = files.map(|files| (files.journal, files.snapshots)).unzip();
        SimServer::open_door(world, terms, server, journal);
        if let Some(snapshots) = snapshots {
            Checkpoints::start(
                world,
                snapshots,
                ServerSigner::new(server.key, server.entropy),
            );
        }
        world.insert_resource(Seats::new(slots, clients));
        world.insert_resource(Receipts::new(slots));
        let start = world.resource::<LocalTimeline>().tick() + 1;
        world.insert_resource(MatchClock::new(start));
        let next = world.resource::<Session>().log().next_tick();
        world.insert_resource(FrameStart(next));
        let driver = BotDriver::new(bots, next, world);
        world.insert_resource(driver);
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

    /// Opens the door of the session of `terms` on the server of `server`, which signs what it
    /// logs; with a `journal`, the log goes into it from now on.
    fn open_door(
        world: &mut World,
        terms: SessionTerms,
        server: &ServerSetup,
        journal: Option<SessionJournal>,
    ) {
        if let Some(journal) = journal {
            world.insert_resource(JournalWatch(journal.watch()));
            world
                .resource_mut::<Session>()
                .keep_journal(Box::new(journal));
        }
        world.insert_resource(ServerSigner::new(server.key, server.entropy));
        world.insert_resource(Door::new(Offering::new(terms, server)));
    }

    /// Keeps the visibility `terms` carry, which each data filter takes its audience from as
    /// the match's entities spawn; an error for terms whose visibility does not read.
    fn set_visibility(world: &mut World, terms: &SessionTerms) -> Result<(), TermsError> {
        let visibility = SessionRules::visibility_of(terms)?;
        world
            .resource_mut::<TeamRooms>()
            .set_spectators(&visibility);
        world.insert_resource(MatchVisibility(visibility));
        Ok(())
    }

    /// Bounds the server's frames for a session of `terms`: a frame advances its clock by the
    /// ticks of the max input delay less one at most, a tick at least, each as long as the
    /// clock's tick now, which a local match's pace may have set before the match started or
    /// restored.
    fn bound_frames(world: &mut World, terms: &SessionTerms) {
        let session = TickRate::new(terms.tick_hz).length();
        let tick = world.resource::<TickDuration>().0;
        assert!(
            PaceSpeed::ALL
                .iter()
                .any(|speed| speed.tick(session) == tick),
            "the server ticks at the session's rate at a speed, not every {tick:?}"
        );
        let burst = terms.max_input_delay.get().saturating_sub(1).max(1);
        let burst = u32::try_from(burst).expect("a max input delay of a LAN session fits u32");
        world
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(tick * burst);
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
    pub fn restore_match(
        world: &mut World,
        restored: RestoredSession,
        packages: &ModePackages,
        server: &ServerSetup,
        bots: ServerBots,
    ) -> Result<(), RestoreMatchError> {
        let RestoredSession {
            private,
            log,
            files: SessionFiles { journal, snapshots },
            ..
        } = restored;
        let terms = log.header().terms.clone();
        SimServer::bound_frames(world, &terms);
        SimServer::set_visibility(world, &terms)
            .map_err(|error| RestoreMatchError::Start(StartError::Terms(error)))?;
        let ticks = log.next_tick();
        let slots = log.slot_count();
        let seeds = private.seed_chain.seeds();
        let latest = log
            .checkpoints()
            .last()
            .map(|record| (record.segment, record.snapshot));
        if let Some((segment, fingerprint)) = latest {
            let snapshot = InputFile::read(
                &snapshots.file(fingerprint),
                StateRegistry::MAX_SNAPSHOT_LEN,
            )
            .map_err(RestoreMatchError::ReadSnapshot)?;
            Session::resume(world, log.rewound(), seeds, packages, segment, &snapshot)
                .map_err(RestoreMatchError::Resume)?;
        } else {
            Session::start(world, log.rewound(), seeds, packages)
                .map_err(RestoreMatchError::Start)?;
        }
        world.insert_resource(JournalWatch(journal.watch()));
        world.session_mut().resume_journal(Box::new(journal));
        let signer = ServerSigner::new(server.key, server.entropy);
        loop {
            Checkpoints::take_again(world, &snapshots, &signer)
                .map_err(RestoreMatchError::WriteSnapshot)?;
            if world.session().log().next_tick() >= ticks {
                break;
            }
            world.run_tick_alone();
            if world.contains_resource::<TickHashes>() {
                record_hash(world);
            }
        }
        SimServer::open_door(world, terms, server, None);
        Checkpoints::start(world, snapshots, signer);
        // The stop may have come after the tick before this boundary, before the save it made due
        // here began.
        Checkpoints::begin(world);
        let players: Vec<PlayerSlot> = world.resource::<Session>().log().player_slots().collect();
        world.resource_scope(|world, mut session: Mut<'_, Session>| {
            let signer = world.resource::<ServerSigner>();
            for &slot in &players {
                signer
                    .serve(&mut session, ServerInput::Disconnected { slot })
                    .expect("the server's own input holds");
            }
        });
        let now = world.resource::<Time<Real>>().elapsed();
        world.insert_resource(Seats::gone(slots, players.into_iter(), now));
        world.insert_resource(Receipts::new(slots));
        let next = world.resource::<Session>().log().next_tick();
        let start = world.resource::<LocalTimeline>().tick() + 1;
        world.insert_resource(MatchClock::resumed(start, next));
        world.insert_resource(FrameStart(next));
        let driver = BotDriver::new(bots, next, world);
        world.insert_resource(driver);
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
                let report = ErrorReport::of(&error).to_string();
                InputMessageRefused {
                    slot: link.slot(),
                    next_tick,
                    error: report.clone(),
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
    for end in &ended {
        let tick = end.tick().get();
        match end.result() {
            MatchResult::Won(team) => {
                info!(tick, team = team.index(), "the match ended: a team won");
            }
            MatchResult::Draw => info!(tick, "the match ended in a draw"),
        }
    }
}
