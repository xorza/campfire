use bevy_app::{App, FixedUpdate, Plugin, RunFixedMainLoop, RunFixedMainLoopSystems, Update};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Has, With, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::common_conditions::{resource_added, resource_exists};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_capabilities::{
    Area, Deaths, MatchEnd, MatchResult, Mode, Owner, Projectile, SeenBy, Team, TeamSet,
};
use campfire_log::LogEvent;
use campfire_math::PlayerSlot;
use campfire_package::ModePackages;
use campfire_protocol::{Applied, ServerSeed, SessionLog};
use campfire_runner::{Session, StartError};
use campfire_sim::{SimTick, StableId, StateHash, Tick, TickRate};
use lightyear::core::tick::TickDuration;
use lightyear::prelude::{
    LocalTimeline, MessageReceiver, MessageSender, NetworkTarget, PredictionTarget, Replicate,
    Unlink, UnlinkReason, VisibilityExt,
};
use tracing::{debug, info, trace, trace_span, warn};

use crate::events::input_logged::InputLogged;
use crate::events::unit_died::UnitDied;
use crate::input_message::InputMessage;
use crate::lobby::Lobby;
use crate::match_clock::MatchClock;
use crate::match_start::MatchStart;
use crate::net_protocol::MatchChannel;

/// Runs a session on a Lightyear server: while a `Lobby` is open, lets players join; then records
/// the packets players send, runs one sim tick in each fixed tick, and sends each client the units
/// its team sees. It hashes the state after a tick only
/// while the world holds `TickHashes`.
#[derive(Debug)]
pub struct SimServer;

/// Which player a client link carries the inputs of, their team, and whether the log refused one
/// of its messages, which ended the link.
#[derive(Component, Debug, Clone, Copy)]
pub struct PlayerLink {
    slot: PlayerSlot,
    team: Team,
    refused: bool,
}

impl PlayerLink {
    pub const fn slot(self) -> PlayerSlot {
        self.slot
    }

    /// Whether the log refused one of its messages: a broken chain or signature, or a limit
    /// passed. A client that follows the rules sends none, so the first one ends the link.
    pub const fn refused(self) -> bool {
        self.refused
    }
}

/// The state hash after each sim tick while the resource exists, from tick 0 when inserted before
/// the match starts: the check `det-ci` makes on every tick, for tests and for a host that looks
/// for a divergence. Production hashes only at checkpoints and at the result.
#[derive(Resource, Debug, Default)]
pub struct TickHashes(Vec<StateHash>);

impl TickHashes {
    pub fn get(&self) -> &[StateHash] {
        &self.0
    }
}

impl Plugin for SimServer {
    fn build(&self, app: &mut App) {
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
        // Lightyear keeps a received message for one frame only, and a frame runs no fixed tick
        // or several, so the inputs are logged in every frame, before its fixed ticks.
        app.add_systems(
            RunFixedMainLoop,
            record_inputs
                .in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop)
                .run_if(resource_exists::<MatchClock>),
        );
        app.add_systems(
            FixedUpdate,
            (
                run_sim_tick,
                record_hash.run_if(resource_exists::<TickHashes>),
                show_units,
                report_deaths,
                announce_end.run_if(resource_added::<MatchEnd>),
            )
                .chain()
                .run_if(sim_tick_due),
        );
    }
}

impl SimServer {
    /// Starts the match of `log`'s header, of the mode `packages` holds, in the next fixed tick,
    /// recording into `log`. `clients` are the links of the players, by slot; each learns its slot
    /// and the start tick. From the first tick on, every unit replicates to the clients whose team
    /// sees it, and the owner's client predicts it, but a projectile or an area.
    pub fn start_match(
        world: &mut World,
        log: SessionLog,
        server_seed: ServerSeed,
        packages: &ModePackages,
        clients: &[Entity],
    ) -> Result<(), StartError> {
        assert_eq!(
            clients.len(),
            log.header().players.len(),
            "one client per player"
        );
        assert_eq!(
            world.resource::<TickDuration>().0,
            TickRate::new(log.header().terms.tick_hz).length(),
            "the server ticks at the session's rate"
        );
        Session::start(world, log, server_seed, packages)?;
        let start = world.resource::<LocalTimeline>().tick() + 1;
        world.insert_resource(MatchClock::new(start));
        for (slot, &client) in (0..).map(PlayerSlot::new).zip(clients) {
            let team = Mode::team_of(world, slot).expect("every player has a team");
            world.entity_mut(client).insert(PlayerLink {
                slot,
                team,
                refused: false,
            });
            world
                .get_mut::<MessageSender<MatchStart>>(client)
                .expect("a client link sends the match start")
                .send::<MatchChannel>(MatchStart {
                    start_tick: start.0,
                    slot,
                });
        }
        Ok(())
    }
}

/// Logs each packet received in this frame, before the next tick runs. A refused packet ends its
/// link: a client that follows the rules sends none, and its chain no longer matches the log's.
fn record_inputs(
    mut commands: Commands<'_, '_>,
    mut links: Query<'_, '_, (Entity, &mut PlayerLink, &mut MessageReceiver<InputMessage>)>,
    mut session: ResMut<'_, Session>,
    mut applied: Local<'_, Vec<Applied>>,
) {
    for (entity, mut link, mut receiver) in &mut links {
        let slot = link.slot.get();
        for message in receiver.receive() {
            if link.refused {
                continue;
            }
            let next_tick = session.log().next_tick();
            let Some(inputs) = message.inputs(link.slot) else {
                warn!(
                    slot,
                    "refused an input message whose frames do not fit its payloads"
                );
                link.refused = true;
                commands.trigger(Unlink {
                    entity,
                    reason: UnlinkReason::UserRequested(Some("broken input message".to_owned())),
                });
                continue;
            };
            if let Err(error) = session.record(inputs.clone(), message.signature(), &mut applied) {
                warn!(slot, next_tick, %error, "refused an input message, which ends the link");
                link.refused = true;
                commands.trigger(Unlink {
                    entity,
                    reason: UnlinkReason::UserRequested(Some(error.to_string())),
                });
                continue;
            }
            for (input, &outcome) in inputs.zip(applied.iter()) {
                match outcome {
                    Applied::At(tick) => InputLogged {
                        slot: link.slot,
                        stamp: Tick::new(input.stamp),
                        tick: Tick::new(tick),
                    }
                    .log(),
                    Applied::Late | Applied::Early => warn!(
                        slot,
                        stamp = input.stamp,
                        next_tick,
                        ?outcome,
                        "logged an input that never takes effect"
                    ),
                }
            }
        }
    }
}

/// The match has started, so this fixed tick runs a sim tick.
fn sim_tick_due(timeline: Res<'_, LocalTimeline>, clock: Option<Res<'_, MatchClock>>) -> bool {
    clock.is_some_and(|clock| clock.sim_tick(timeline.tick()).is_some())
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
    trace!(tick = hashes.0.len(), %hash, "hashed the state");
    hashes.0.push(hash);
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

/// The units not replicated yet, with their owner if they have one, and whether they are
/// projectiles or areas.
type NewUnits<'w, 's> = Query<
    'w,
    's,
    (Entity, Option<&'static Owner>, Has<Projectile>, Has<Area>),
    (With<Team>, Without<Replicate>),
>;

/// After a sim tick, replicates each new unit, predicted by its owner's client unless it is a
/// projectile or an area, which the server's sim alone runs, and shows each unit whose seers changed to
/// exactly the clients whose team sees it. A unit is hidden in the tick it replicates in, so a
/// client never receives a unit its team did not see: a projectile shows where it flies, not
/// where its source stands. Without vision no unit has `SeenBy`, and every client receives every
/// unit.
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
            .and_then(|owner| links.iter().find(|(_, link)| link.slot == owner.slot()));
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
        let visible = seen.contains(player.team);
        debug!(
            unit = id.get(),
            slot = player.slot.get(),
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
