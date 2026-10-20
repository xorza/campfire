use bevy_app::{
    App, FixedUpdate, Last, Plugin, PostUpdate, RunFixedMainLoop, RunFixedMainLoopSystems, Update,
};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, QueryState};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::common_conditions::resource_exists;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use bevy_state::app::AppExtStates;
use bevy_state::condition::in_state;
use bevy_time::{Real, Time, Virtual};
use campfire_common::ErrorReport;
use campfire_common::{Seat, Tick};
use campfire_declarations::CapabilitySet;
use campfire_log::LogEvent;
use campfire_math::TickRate;
use campfire_mode::MatchEnd;
use campfire_net_connection::Disconnect;
use campfire_net_link::{LinkOf, LinkSystems};
use campfire_net_messages::MessageReceiver;
use campfire_net_replication::{ReplicationSender, RoomAllocator, RoomPlugin};
use campfire_net_stack::ServerPlugins;
use campfire_net_timeline::LocalTimeline;
use campfire_protocol::{Applied, Controller, InputError, PlayerPacket};
use campfire_runner::{Session, SessionWorld};
use campfire_script::ScriptPrints;
use campfire_sim::SimTick;
use campfire_units::Deaths;
use tracing::trace_span;

use crate::close_code::CloseCode;
use crate::events::input_logged::InputLogged;
use crate::events::input_message_refused::InputMessageRefused;
use crate::events::input_message_unfit::InputMessageUnfit;
use crate::events::input_never_applied::{InputNeverApplied, Unapplied};
use crate::events::log_filled::LogFilled;
use crate::events::match_ended::MatchEnded;
use crate::events::save_refused::SaveRefused;
use crate::events::script_printed::ScriptPrinted;
use crate::events::server_frame::ServerFrame;
use crate::events::ticks_caught_up::TicksCaughtUp;
use crate::events::time_dropped::TimeDropped;
use crate::events::unit_died::UnitDied;
use crate::faults::WorkerFaults;
use crate::faults::fault::Fault;
use crate::hosting::Hosting;
use crate::match_clock::MatchClock;

use crate::net_protocol::NetProtocolPlugin;
use crate::net_protocol::input_message::InputMessage;
use crate::net_protocol::save_command::SaveCommand;
use crate::sim_server::ack_sender::AckSender;
use crate::sim_server::bot_driver::BotDriver;
use crate::sim_server::checkpoints::Checkpoints;
use crate::sim_server::data_filter::DataFilters;
use crate::sim_server::door::Door;
use crate::sim_server::journal_watch::JournalWatch;
use crate::sim_server::kind_shows::{KindShows, ShowSystems};
use crate::sim_server::lobby::Lobby;
use crate::sim_server::offering::{Dismissing, Offered, Offering};
use crate::sim_server::pending_restore::PendingRestore;
use crate::sim_server::player_link::PlayerLink;
use crate::sim_server::receipts::Receipts;
use crate::sim_server::server_phase::ServerPhase;
use crate::sim_server::sim_server_world::{SimServerWorld, SimServerWorldCrate};
use crate::sim_server::spectators::Spectators;
use crate::sim_server::team_rooms::TeamRooms;
use crate::sim_server::viewer::Viewer;

pub(crate) mod ack_sender;
pub(crate) mod bot_driver;
pub(crate) mod checkpoint_thread;
pub(crate) mod checkpoints;
pub(crate) mod data_filter;
pub(crate) mod door;
pub(crate) mod error;
pub(crate) mod journal_watch;
pub(crate) mod kind_shows;
pub(crate) mod lobby;
pub(crate) mod match_visibility;
pub(crate) mod offering;
pub(crate) mod pending_restore;
pub(crate) mod player_link;
pub(crate) mod receipts;
pub(crate) mod seat_bot_file;
pub(crate) mod seat_links;
pub(crate) mod server_bots;
pub(crate) mod server_dir;
pub(crate) mod server_layout;
pub(crate) mod server_match;
pub(crate) mod server_phase;
pub(crate) mod server_setup;
pub(crate) mod session_dir;
pub(crate) mod session_journal;
pub(crate) mod sim_server_plugins;
pub(crate) mod sim_server_world;
pub(crate) mod snapshot_dir;
pub(crate) mod spectators;
pub(crate) mod team_rooms;
#[cfg(feature = "internals")]
pub(crate) mod tick_hashes;
pub(crate) mod viewer;

/// Runs a session on a network stack server: while a `Lobby` is open, lets players join, or
/// restores a `PendingRestore`; then records the packets players send, runs one sim tick in each
/// fixed tick, and sends each client the units its team sees. It hashes the state after a tick only
/// while the world holds `TickHashes`. It adds the network stack's server at its `tick_rate`, the
/// protocol, and the replication to each new link; `SimServerPlugins` adds it.
#[derive(Debug)]
pub(crate) struct SimServerPlugin {
    pub(crate) tick_rate: TickRate,
    /// The mode's declared capabilities, whose components alone replicate.
    pub(crate) capabilities: CapabilitySet,
    pub(crate) hosting: Hosting,
}

impl Plugin for SimServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ServerPlugins {
            tick_rate: self.tick_rate,
        });
        app.add_plugins((
            NetProtocolPlugin {
                capabilities: self.capabilities,
            },
            RoomPlugin,
        ));
        DataFilters::register(app, self.capabilities);
        KindShows::register(app);
        let rooms = TeamRooms::new(&mut app.world_mut().resource_mut::<RoomAllocator>());
        app.insert_resource(rooms);
        app.insert_resource(self.hosting);
        app.register_required_components_with::<LinkOf, ReplicationSender>(|| ReplicationSender);
        app.init_resource::<WorkerFaults>();
        app.init_state::<ServerPhase>();
        app.configure_sets(
            Update,
            (
                ServerFrameSystems::Intake,
                ServerFrameSystems::Door,
                ServerFrameSystems::SessionUpkeep,
                ServerFrameSystems::Receipts,
                ServerFrameSystems::Saves,
            )
                .chain(),
        );
        app.add_systems(
            Update,
            (
                PendingRestore::run.run_if(in_state(ServerPhase::Restoring)),
                Spectators::choose_teams,
            )
                .chain()
                .in_set(ServerFrameSystems::Intake),
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
                .run_if(in_state(ServerPhase::Lobby))
                .in_set(ServerFrameSystems::Door),
        );
        // Each system that logs checks that the session runs as it starts, as another system of
        // the frame may end it.
        app.add_systems(
            Update,
            (Offering::offer_links::<Door>, Door::take_joins, Door::watch)
                .chain()
                .after(Lobby::start_when_full)
                .distributive_run_if(in_state(ServerPhase::Running))
                .distributive_run_if(session_running)
                .in_set(ServerFrameSystems::Door),
        );
        app.add_systems(
            Update,
            (
                Checkpoints::end.run_if(resource_exists::<Checkpoints>),
                JournalWatch::warn_slow.run_if(resource_exists::<JournalWatch>),
            )
                .chain()
                .in_set(ServerFrameSystems::SessionUpkeep),
        );
        app.add_systems(
            Update,
            Receipts::give
                .run_if(in_state(ServerPhase::Running))
                .run_if(session_running)
                .in_set(ServerFrameSystems::Receipts),
        );
        app.add_systems(
            Update,
            Self::take_save_commands
                .run_if(in_state(ServerPhase::Running))
                .run_if(session_running)
                .in_set(ServerFrameSystems::Saves),
        );
        app.add_observer(Spectators::leave);
        Viewer::follow_links(app);
        Viewer::follow_out_of_play(app);
        app.add_systems(Last, Dismissing::tell);
        app.add_systems(Last, JournalWatch::flush.run_if(resource_exists::<Session>));
        app.add_systems(
            PostUpdate,
            (Dismissing::end, Offered::close_silent).after(LinkSystems::Send),
        );
        // The network stack keeps a received message for one frame only, and a frame runs no fixed
        // tick or several, so the inputs are logged in every frame, before its fixed ticks; a frame
        // that runs several is logged, as an input that missed its start waits for all of them.
        app.add_systems(
            RunFixedMainLoop,
            (
                record_inputs.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
                report_catch_up.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop),
                report_dropped_time.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
            )
                .distributive_run_if(in_state(ServerPhase::Running))
                .distributive_run_if(session_running),
        );
        app.add_systems(
            FixedUpdate,
            (
                BotDriver::drive,
                run_sim_tick,
                // Right after the tick, before any entry of the next.
                Self::at_boundary,
                report_deaths,
                report_prints,
                report_end,
            )
                .chain()
                .in_set(SimTickSystems),
        );
        #[cfg(feature = "internals")]
        app.add_systems(
            FixedUpdate,
            tick_hashes::TickHashes::record
                .after(run_sim_tick)
                .before(Self::at_boundary)
                .in_set(SimTickSystems),
        );
        app.configure_sets(
            FixedUpdate,
            SimTickSystems
                .run_if(in_state(ServerPhase::Running))
                .run_if(sim_tick_due),
        );
        app.configure_sets(
            FixedUpdate,
            ShowSystems
                .in_set(SimTickSystems)
                .after(Self::at_boundary)
                .before(report_deaths),
        );
    }
}

/// The steps of the server's frame in `Update`, in their order, after the network stack's: what
/// comes in from outside the session, a restore waiting or a spectator's choice; the door, which
/// seats players, before the match in the lobby and after it at the door; the session's upkeep, its
/// checkpoints and journal; the receipts of what the journal made durable; and the players' save
/// commands, last, as a load ends the run. An app's own system of `Update` goes after them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerFrameSystems {
    Intake,
    Door,
    SessionUpkeep,
    Receipts,
    Saves,
}

/// The systems of a fixed tick that runs a sim tick, whose condition is checked once, before the
/// first of them, so the tick runs whole even when it ends the session.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SimTickSystems;

/// The save commands' links, each with its player.
type SaveLinks = QueryState<(
    &'static PlayerLink,
    &'static mut MessageReceiver<SaveCommand>,
)>;

impl SimServerPlugin {
    /// At the boundary before the next tick: ends the session when its log is full, which the
    /// room the log keeps lets it do; else begins the checkpoint due there, as
    /// `Checkpoints::begin` does when the session keeps them, and ends the session when that
    /// gives that it ends there. A checkpoint still on its thread that is not written then is a
    /// fault, as the log can not be published.
    fn at_boundary(world: &mut World) {
        let session = world.session();
        let ends = if session.is_log_full() {
            LogFilled {
                tick: session.log().next_tick(),
            }
            .log();
            true
        } else {
            world.contains_resource::<Checkpoints>() && Checkpoints::begin(world)
        };
        if ends && let Err(error) = world.end_session() {
            world
                .resource_mut::<WorkerFaults>()
                .report(Fault::Snapshot(error));
        }
    }

    /// Takes the save commands of the seated players, as `Checkpoints::take` does, and loads the
    /// save a load names; each refused one is logged. A load ends the run the commands after it
    /// were given in, so the frame takes none of them.
    fn take_save_commands(
        world: &mut World,
        (mut links, mut taken): (Local<'_, SaveLinks>, Local<'_, Vec<SaveCommand>>),
    ) {
        for (link, mut receiver) in links.iter_mut(world) {
            if !link.is_refused() {
                taken.extend(receiver.receive());
            }
        }
        for command in taken.drain(..) {
            match Checkpoints::take(world, command) {
                Ok(None) => {}
                Ok(Some(segment)) => {
                    world
                        .load_session_save(segment)
                        .expect("no checkpoint is on the thread");
                    break;
                }
                Err(refusal) => SaveRefused { refusal }.log(),
            }
        }
    }
}

/// Logs each packet received in this frame, before the next tick runs, and tells its player where
/// each of its inputs takes effect. A refused packet ends its link: a client that follows the
/// rules sends none, and its chain no longer matches the log's. The links go in the order of
/// their seats, as the log's order of one frame's inputs must not follow the query's.
fn record_inputs(
    mut commands: Commands<'_, '_>,
    mut links: Query<'_, '_, (Entity, &PlayerLink, &mut MessageReceiver<InputMessage>)>,
    mut acks: AckSender<'_, '_>,
    mut session: ResMut<'_, Session>,
    mut frame: ResMut<'_, FrameStart>,
    mut applied: Local<'_, Vec<Applied>>,
    mut order: Local<'_, Vec<(Seat, Entity)>>,
) {
    frame.0 = session.log().next_tick();
    ServerFrame { tick: frame.0 }.log();
    order.extend(links.iter().map(|(entity, link, ..)| (link.seat(), entity)));
    order.sort_unstable();
    for (_, entity) in order.drain(..) {
        let (entity, &(mut link), mut receiver) =
            links.get_mut(entity).expect("a link the query just gave");
        for message in receiver.receive() {
            if link.is_refused() {
                continue;
            }
            let next_tick = session.log().next_tick();
            let seat = link.seat();
            let Some(inputs) = message.inputs() else {
                InputMessageUnfit { seat }.log();
                link = refuse_link(&mut commands, entity, link);
                continue;
            };
            let packet = PlayerPacket {
                seat,
                inputs: inputs.clone(),
                signature: message.signature(),
            };
            if let Err(error) = session.record(packet, &mut applied) {
                InputMessageRefused {
                    seat: link.seat(),
                    next_tick,
                    error: ErrorReport::of(&error).to_string(),
                }
                .log();
                // Not the client's doing: the session ends at the next boundary.
                if error == InputError::LogFull {
                    continue;
                }
                link = refuse_link(&mut commands, entity, link);
                continue;
            }
            let Some(Controller::Player { chain, .. }) = session.log().controller(link.seat())
            else {
                unreachable!("a packet the log took is of a seat a player controls");
            };
            let first_seq = chain.next_seq() - u64::try_from(applied.len()).expect("a count fits");
            acks.send(entity, first_seq, &applied);
            for (((stamp, _), &outcome), seq) in inputs.zip(applied.iter()).zip(first_seq..) {
                match Unapplied::try_from(outcome) {
                    Err(tick) => InputLogged {
                        seat: link.seat(),
                        seq,
                        stamp,
                        tick,
                    }
                    .log(),
                    Ok(outcome) => InputNeverApplied {
                        seat: link.seat(),
                        stamp,
                        next_tick,
                        outcome,
                    }
                    .log(),
                }
            }
        }
    }
}

/// Refuses `link`, the player link of `entity`, whose packet broke the rules: it carries no more
/// of its seat's inputs, and its connection closes. The link as refused.
fn refuse_link(commands: &mut Commands<'_, '_>, entity: Entity, link: PlayerLink) -> PlayerLink {
    let refused = link.into_refused();
    commands.entity(entity).insert(refused);
    commands.trigger(Disconnect {
        entity,
        reason: CloseCode::InputRefused.app(),
    });
    refused
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

/// The running match's clock has reached its first tick and its session has not ended, so this
/// fixed tick runs a sim tick. A condition's parameters are checked whatever the conditions
/// before it give, so a phase without a match reads none.
fn sim_tick_due(
    timeline: Res<'_, LocalTimeline>,
    clock: Option<Res<'_, MatchClock>>,
    session: Option<Res<'_, Session>>,
) -> bool {
    clock.is_some_and(|clock| clock.sim_tick(timeline.tick()).is_some()) && session_running(session)
}

/// The running match's session has not ended, which it may within a frame; none before the
/// match begins.
fn session_running(session: Option<Res<'_, Session>>) -> bool {
    session.is_some_and(|session| session.log().result().is_none())
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

/// What a replayed tick of a restore leaves: a test's hash of it, which a test's build keeps.
#[cfg_attr(
    not(feature = "internals"),
    expect(
        clippy::missing_const_for_fn,
        reason = "a test's build keeps the tick's hash, which no const fn computes"
    )
)]
fn replayed(
    #[cfg_attr(
        not(feature = "internals"),
        expect(unused_variables, reason = "only a test's build keeps a tick's hash")
    )]
    world: &mut World,
) {
    #[cfg(feature = "internals")]
    tick_hashes::TickHashes::record(world);
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

/// Logs what the scripts printed in the tick just run.
fn report_prints(tick: Res<'_, SimTick>, prints: Option<Res<'_, ScriptPrints>>) {
    let Some(prints) = prints else {
        return;
    };
    let ran = Tick::new(tick.start().get() - 1);
    if prints.tick() != ran {
        return;
    }
    for print in prints.iter() {
        ScriptPrinted::of(ran, print).log();
    }
}

/// Logs the end of the match the tick just run ended, which reaches the clients on the match's
/// entity.
fn report_end(ended: Query<'_, '_, &MatchEnd, Added<MatchEnd>>) {
    for &end in &ended {
        MatchEnded::of(end).log();
    }
}
