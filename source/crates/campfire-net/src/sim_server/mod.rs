use std::fs;

use bevy_app::{
    App, FixedUpdate, Last, Plugin, PostUpdate, RunFixedMainLoop, RunFixedMainLoopSystems, Update,
};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Has, With, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::{
    resource_added, resource_exists, resource_exists_and_changed,
};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::{Mut, World};
use bevy_time::{Real, Time, Virtual};
use campfire_capabilities::{
    Area, Deaths, MatchEnd, MatchResult, Mode, Owner, Projectile, Relations, SeenBy, Team, TeamSet,
};
use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogEvent;
use campfire_package::ModePackages;
use campfire_protocol::{Applied, Outcome, ServerInput, ServerSeeds, SessionLog, SessionTerms};
use campfire_runner::{Session, StartError};
use campfire_sim::{SimTick, StableId, TickRate};
use campfire_store::DurableError;
use lightyear::core::tick::TickDuration;
use lightyear::prelude::{
    LinkSystems, LocalTimeline, MessageReceiver, MessageSender, NetworkTarget, PredictionTarget,
    Replicate, Unlink, UnlinkReason, VisibilityExt,
};
use tracing::{debug, info, trace, trace_span};

use crate::error::RestoreMatchError;
use crate::events::input_logged::InputLogged;
use crate::events::input_message_refused::InputMessageRefused;
use crate::events::input_message_unfit::InputMessageUnfit;
use crate::events::input_never_applied::{InputNeverApplied, Unapplied};
use crate::events::ticks_caught_up::TicksCaughtUp;
use crate::events::time_dropped::TimeDropped;
use crate::events::unit_died::UnitDied;
use crate::faults::Faults;
use crate::faults::fault::Fault;
use crate::input_message::InputMessage;
use crate::match_clock::MatchClock;
use crate::match_start::MatchStart;
use crate::net_protocol::MatchChannel;
use crate::pace::PaceSpeed;
use crate::sim_server::bot_driver::BotDriver;
use crate::sim_server::checkpoints::Checkpoints;
use crate::sim_server::door::Door;
use crate::sim_server::journal_watch::JournalWatch;
use crate::sim_server::lobby::Lobby;
use crate::sim_server::offering::{Offering, Superseding};
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::receipts::Receipts;
use crate::sim_server::seats::Seats;
use crate::sim_server::server_bots::ServerBots;
use crate::sim_server::server_setup::ServerSetup;
use crate::sim_server::server_signer::ServerSigner;
use crate::sim_server::session_dir::{RestoredSession, SessionFiles};
use crate::sim_server::session_journal::SessionJournal;
use crate::sim_server::tick_hashes::TickHashes;

pub(crate) mod bot_driver;
pub(crate) mod checkpoint_thread;
pub(crate) mod checkpoints;
pub(crate) mod door;
pub(crate) mod journal_watch;
pub(crate) mod key_file;
pub(crate) mod lobby;
pub(crate) mod offering;
pub(crate) mod player_link;
pub(crate) mod receipts;
pub(crate) mod seats;
pub(crate) mod server_bots;
pub(crate) mod server_dir;
pub(crate) mod server_exit;
pub(crate) mod server_setup;
pub(crate) mod server_signer;
pub(crate) mod session_dir;
pub(crate) mod session_journal;
pub(crate) mod tick_hashes;

/// Runs a session on a Lightyear server: while a `Lobby` is open, lets players join; then records
/// the packets players send, runs one sim tick in each fixed tick, and sends each client the units
/// its team sees. It hashes the state after a tick only while the world holds `TickHashes`.
#[derive(Debug)]
pub struct SimServer;

impl Plugin for SimServer {
    fn build(&self, app: &mut App) {
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
        app.add_systems(Last, Superseding::tell);
        app.add_systems(PostUpdate, Superseding::end.after(LinkSystems::Send));
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
                show_units,
                report_deaths,
                announce_end.run_if(resource_added::<MatchEnd>),
                send_relations.run_if(resource_exists_and_changed::<Relations>),
            )
                .chain()
                .run_if(sim_tick_due),
        );
    }
}

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
    pub(crate) fn start_match(
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
            let team = Mode::team_of(world, slot).expect("every player has a team");
            world.entity_mut(client).insert(PlayerLink::new(slot, team));
            world
                .get_mut::<MessageSender<MatchStart>>(client)
                .expect("a client link sends the match start")
                .send::<MatchChannel>(MatchStart {
                    start_tick: start.0,
                    first: next,
                    slot,
                    chain: None,
                    loaded: false,
                });
        }
        Ok(())
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
        let ticks = log.next_tick();
        let slots = log.slot_count();
        let seeds = private.seed_chain.seeds();
        let latest = log
            .checkpoints()
            .last()
            .map(|record| (record.segment, record.snapshot));
        if let Some((segment, fingerprint)) = latest {
            let snapshot =
                fs::read(snapshots.file(fingerprint)).map_err(RestoreMatchError::ReadSnapshot)?;
            Session::resume(world, log.rewound(), seeds, packages, segment, &snapshot)
                .map_err(RestoreMatchError::Resume)?;
        } else {
            Session::start(world, log.rewound(), seeds, packages)
                .map_err(RestoreMatchError::Start)?;
        }
        world.insert_resource(JournalWatch(journal.watch()));
        world
            .resource_mut::<Session>()
            .resume_journal(Box::new(journal));
        let signer = ServerSigner::new(server.key, server.entropy);
        loop {
            Checkpoints::take_again(world, &snapshots, &signer)
                .map_err(RestoreMatchError::WriteSnapshot)?;
            if world.resource::<Session>().log().next_tick() >= ticks {
                break;
            }
            Session::run_tick(world);
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

    /// Whether every slot a player controlled has left, and no player holds a link: a session
    /// that started then ends.
    pub fn players_gone(world: &World) -> bool {
        Door::empty(world)
    }

    /// Ends the session in `world` before the next tick as `outcome`: logs the record of the
    /// checkpoint on its thread first; then the result, which the server key signs; reveals the
    /// seed; and from then on runs no tick and logs nothing, as each such system checks that the
    /// session runs. The server then publishes the log. An error when the checkpoint's snapshot
    /// was not written, as the log can then not be published: the session ends with no result.
    pub fn end_session(world: &mut World, outcome: Outcome) -> Result<(), DurableError> {
        if world.contains_resource::<Checkpoints>() {
            Checkpoints::settle(world)?;
            world.remove_resource::<Checkpoints>();
        }
        world.resource_scope(|world, mut session: Mut<'_, Session>| {
            let result = session.result(world, outcome);
            let id = session.log().session_id();
            let signature = world.resource::<ServerSigner>().sign_result(&result, id);
            session
                .record_result(result, &signature)
                .expect("the server's own result holds");
            session.reveal_seed();
        });
        Ok(())
    }

    /// Loads the save that starts segment `segment` in the session in `world`: logs the record of
    /// the checkpoint on its thread first, then goes back in the log, as `SessionLog::load`
    /// says, which the journal logs. The match then runs no tick and logs nothing: its server
    /// starts again on its data, which restores the match from the save, once `reload_wanted`
    /// says so. An error when the checkpoint's snapshot was not written.
    pub fn load(world: &mut World, segment: u32) -> Result<(), DurableError> {
        Checkpoints::settle(world)?;
        world
            .resource_mut::<Session>()
            .load(segment)
            .expect("a save of the session's log");
        world.insert_resource(Reload);
        Ok(())
    }

    /// Whether the session in `world` went back to a save, and its server is to start again on
    /// its data, which restores the match from it.
    pub fn reload_wanted(world: &World) -> bool {
        world.contains_resource::<Reload>()
    }

    /// Waits for the checkpoint on its thread in the session in `world`, whose server keeps its
    /// files, and logs its record; reports the fault when its snapshot was not written.
    pub fn settle_checkpoint(world: &mut World) {
        if let Err(error) = Checkpoints::settle(world) {
            world
                .resource_mut::<Faults>()
                .report(Fault::Snapshot(error));
        }
    }

    /// Makes a checkpoint due at the boundary before `tick` in the session in `world`, whose
    /// server keeps its files.
    pub fn request_checkpoint(world: &mut World, tick: Tick) {
        world.resource_mut::<Checkpoints>().request(tick);
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
}

/// Logs each packet received in this frame, before the next tick runs. A refused packet ends its
/// link: a client that follows the rules sends none, and its chain no longer matches the log's.
fn record_inputs(
    mut commands: Commands<'_, '_>,
    mut links: Query<'_, '_, (Entity, &mut PlayerLink, &mut MessageReceiver<InputMessage>)>,
    mut session: ResMut<'_, Session>,
    mut frame: ResMut<'_, FrameStart>,
    mut applied: Local<'_, Vec<Applied>>,
) {
    frame.0 = session.log().next_tick();
    for (entity, mut link, mut receiver) in &mut links {
        for message in receiver.receive() {
            if link.refused() {
                continue;
            }
            let next_tick = session.log().next_tick();
            let Some(inputs) = message.inputs(link.slot()) else {
                InputMessageUnfit { slot: link.slot() }.log();
                link.refuse();
                commands.trigger(Unlink {
                    entity,
                    reason: UnlinkReason::UserRequested(Some("broken input message".to_owned())),
                });
                continue;
            };
            if let Err(error) = session.record(inputs.clone(), message.signature(), &mut applied) {
                InputMessageRefused {
                    slot: link.slot(),
                    next_tick,
                    error: error.to_string(),
                }
                .log();
                link.refuse();
                commands.trigger(Unlink {
                    entity,
                    reason: UnlinkReason::UserRequested(Some(error.to_string())),
                });
                continue;
            }
            for (input, &outcome) in inputs.zip(applied.iter()) {
                let outcome = match outcome {
                    Applied::At(tick) => {
                        InputLogged {
                            slot: link.slot(),
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
    Session::run_tick(world);
}

fn record_hash(world: &mut World) {
    let hash = world.resource::<Session>().state_hash(world);
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

/// Logs the end of the match the tick just run ended, and tells each player's client.
fn announce_end(
    end: Res<'_, MatchEnd>,
    mut links: Query<'_, '_, &mut MessageSender<MatchEnd>, With<PlayerLink>>,
) {
    let tick = end.tick().get();
    match end.result() {
        MatchResult::Won(team) => info!(tick, team = team.index(), "the match ended: a team won"),
        MatchResult::Draw => info!(tick, "the match ended in a draw"),
    }
    for mut sender in &mut links {
        sender.send::<MatchChannel>(*end);
    }
}

/// Tells each player's client how the teams regard each other, once as the match starts, and
/// again in each tick a script changes it.
fn send_relations(
    relations: Res<'_, Relations>,
    mut links: Query<'_, '_, &mut MessageSender<Relations>, With<PlayerLink>>,
) {
    for mut sender in &mut links {
        sender.send::<MatchChannel>(relations.clone());
    }
}

/// The units not replicated yet, with their owner if they have one, and whether they are
/// projectiles or areas.
type NewUnits<'w, 's> = Query<
    'w,
    's,
    (Entity, Option<&'static Owner>, Has<Projectile>, Has<Area>),
    (With<Team>, Without<Replicate>),
>;

/// After a sim tick, replicates each new unit, predicted by its owner's client unless it is a
/// projectile or an area, which the server's sim alone runs, and shows each unit whose seers
/// changed to exactly the clients whose team sees it. A unit is hidden in the tick it replicates
/// in, so a client never receives a unit its team did not see: a projectile shows where it flies,
/// not where its source stands. Without vision no unit has `SeenBy`, and every client receives
/// every unit.
fn show_units(
    links: Query<'_, '_, (Entity, &PlayerLink)>,
    new: NewUnits<'_, '_>,
    changed: Query<'_, '_, (Entity, &StableId, &SeenBy), Changed<SeenBy>>,
    mut commands: Commands<'_, '_>,
) {
    for (unit, owner, projectile, area) in &new {
        let mut replicated = commands.entity(unit);
        replicated.insert(Replicate::to_clients(NetworkTarget::All));
        let owner = owner
            .filter(|_| !projectile && !area)
            .and_then(|owner| links.iter().find(|(_, link)| link.slot() == owner.slot()));
        if let Some((link, _)) = owner {
            replicated.insert(PredictionTarget::manual(vec![link]));
        }
    }
    for (unit, &id, &seen) in &changed {
        show(&mut commands, &links, unit, id, seen.get());
    }
}

fn show(
    commands: &mut Commands<'_, '_>,
    links: &Query<'_, '_, (Entity, &PlayerLink)>,
    unit: Entity,
    id: StableId,
    seen: TeamSet,
) {
    for (link, player) in links {
        let visible = seen.contains(player.team());
        debug!(
            unit = id.get(),
            slot = player.slot().get(),
            visible,
            "set a unit's visibility"
        );
        if visible {
            commands.gain_visibility(unit, link);
        } else {
            commands.lose_visibility(unit, link);
        }
    }
}
