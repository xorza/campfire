use std::collections::BTreeMap;
use std::mem;
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use bevy_time::{Fixed, Real, Time};
use campfire_capabilities::{MatchProbe, ProbeColumns, ProbePage, ProbeState, ProbeUnits};
use campfire_common::{PlayerSlot, StableNumber};
use campfire_console::{
    ConsoleAnswer, ConsoleAsk, ConsoleRefusal, Detail, GameSpeed, NearestNames, PaceAsk,
    PaceStatus, PlayerStatus, Reply, Request, RequestId, RunAsk, RunCondition, RunStop, RunTo,
    SessionPhase, SessionStatus, UnitActivity, UnitSelection, UnitTable, UnitsAsk,
};
use campfire_mode::MatchEnd;
use campfire_players::{Player, PlayerResources, ResourceBook};
use campfire_runner::Session;
use campfire_sim::{EntityIndex, MatchRate, SimTick, SimWorld, StableId, StateRegistry};
use campfire_units::UnitTypeBook;
use campfire_values::{Team, TypeScope};

use crate::console::console_stdio::SEEN_POISONED;
use crate::console::log_seen::LogSeen;
use crate::pace::{Pace, PaceSpeed};
use crate::server::server_phase::ServerPhase;

/// A match's console: the requests its transport queued, which the frame answers between two
/// ticks as the world stands, and the replies the transport sends; and a local match's run, which
/// plays at the pace's fastest speed and pauses after the tick it ends at, as the local server
/// holds it after each tick (console.md, A console's local match starts paused). Its transport,
/// the standard streams or the client's link, fills and drains its queues.
#[derive(Resource, Debug)]
pub struct MatchConsole {
    /// A local match's pace; none on a dedicated server, which runs no run.
    pace: Option<Arc<Pace>>,
    seen: Arc<Mutex<LogSeen>>,
    queues: Mutex<ConsoleQueues>,
}

/// What the console holds between its transport and its answers.
#[derive(Debug, Default)]
struct ConsoleQueues {
    requests: Vec<Request>,
    replies: Vec<Reply>,
    run: Option<ConsoleRun>,
}

/// A run under way: the request it answers, none for the hold of a match that starts paused;
/// where it ends; the wall time it ends at, by the app's real clock; the speed before it; and
/// whether a cancel ended it.
#[derive(Debug)]
struct ConsoleRun {
    id: Option<RequestId>,
    until: RunUntil,
    deadline: Option<Duration>,
    before: PaceSpeed,
    cancelled: bool,
}

/// Where a run ends, its names resolved as it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunUntil {
    /// After this tick ran.
    Tick(u64),
    Unit(StableId, ProbeState),
    /// The log's tap saw the event it watches.
    Event,
    /// More script calls failed than this count.
    ScriptFailure(u64),
    MatchEnd,
}

/// What a run's request starts: a run, or an answer at once.
#[derive(Debug)]
enum RunStart {
    Runs(ConsoleRun),
    Answered(ConsoleAnswer),
}

impl MatchConsole {
    /// A match's console, of the local match's `pace`, none on a dedicated server, reading what
    /// the log's tap saw from `seen`.
    pub const fn new(pace: Option<Arc<Pace>>, seen: Arc<Mutex<LogSeen>>) -> Self {
        Self {
            pace,
            seen,
            queues: Mutex::new(ConsoleQueues {
                requests: Vec::new(),
                replies: Vec::new(),
                run: None,
            }),
        }
    }

    /// Holds the match paused after its first tick, as a console's local match starts
    /// (console.md, A console's local match starts paused); a run asked before replaces the hold.
    pub fn hold_first_tick(&self) {
        let pace = self
            .pace
            .as_ref()
            .expect("a local match's console holds its first tick");
        self.queues().run = Some(ConsoleRun {
            id: None,
            until: RunUntil::Tick(0),
            deadline: None,
            before: pace.speed(),
            cancelled: false,
        });
    }

    /// Queues `requests`, which the next answer takes in their order.
    pub fn push(&self, requests: impl IntoIterator<Item = Request>) {
        self.queues().requests.extend(requests);
    }

    /// Moves the replies made since the last take into `replies`, in their order.
    pub fn take_replies(&self, replies: &mut Vec<Reply>) {
        replies.append(&mut self.queues().replies);
    }

    /// Answers each request queued, in its order, as the world stands between two ticks; a run
    /// starts, and its reply comes as it ends. A system of read-only parameters, so a read moves
    /// no change tick (console.md, A read changes nothing).
    pub fn answer(world: &World) {
        let console = world.resource::<Self>();
        let mut queues = console.queues();
        if queues.requests.is_empty() {
            return;
        }
        let tick = Self::ran(world);
        for request in mem::take(&mut queues.requests) {
            let answer = match request.ask {
                ConsoleAsk::Status => Some(ConsoleAnswer::Status(console.status(world))),
                ConsoleAsk::Units(ask) => Some(Self::units(world, &ask)),
                ConsoleAsk::Run(ask) => console.start(world, &mut queues.run, request.id, &ask),
                ConsoleAsk::Cancel(run) => {
                    if let Some(under_way) = &mut queues.run
                        && under_way.id == Some(run)
                    {
                        under_way.cancelled = true;
                    }
                    Some(ConsoleAnswer::Cancelled)
                }
                ConsoleAsk::Pace(ask) => Some(console.pace(&mut queues.run, ask)),
                ConsoleAsk::Screenshot(_) | ConsoleAsk::Camera(_) => {
                    Some(ConsoleAnswer::Refused(ConsoleRefusal::NotClient))
                }
            };
            if let Some(answer) = answer {
                queues.replies.push(Reply {
                    id: request.id,
                    tick,
                    answer,
                });
            }
        }
    }

    /// Ends the run under way once it reached its end, after a tick or as a frame starts, as the
    /// local server holds it: the time the frame holds for more ticks goes, so none runs after
    /// it, and the pace pauses at the speed before the run; its reply names the tick it ended at
    /// and why.
    pub fn hold(world: &mut World) {
        let Some(console) = world.get_resource::<Self>() else {
            return;
        };
        let Some(stop) = console.stop(world) else {
            return;
        };
        let mut fixed = world.resource_mut::<Time<Fixed>>();
        let held = fixed.overstep();
        fixed.discard_overstep(held);
        let tick = Self::ran(world);
        let console = world.resource::<Self>();
        let mut queues = console.queues();
        let run = queues.run.take().expect("a run under way stops");
        console.seen().unwatch();
        console
            .pace
            .as_ref()
            .expect("a run is a local match's")
            .end_run(run.before);
        if let Some(id) = run.id {
            queues.replies.push(Reply {
                id,
                tick,
                answer: ConsoleAnswer::Ran(stop),
            });
        }
    }

    fn queues(&self) -> MutexGuard<'_, ConsoleQueues> {
        self.queues
            .lock()
            .expect("no code panics holding a console's queues")
    }

    fn seen(&self) -> MutexGuard<'_, LogSeen> {
        self.seen.lock().expect(SEEN_POISONED)
    }

    /// The last tick `world` ran, none before its first or before a match.
    fn ran(world: &World) -> Option<u64> {
        let tick = world.get_resource::<SimTick>()?;
        tick.start().get().checked_sub(1)
    }

    /// The entity of the unit `id` of `world`, none before a match or for a unit it lacks.
    fn entity_of(world: &World, id: StableId) -> Option<Entity> {
        world.get_resource::<EntityIndex>()?.get(id)
    }

    /// Whether `world`'s match ended, or its session, which then runs no tick more: as the log
    /// filled, or a checkpoint past the seed chain ended it.
    fn has_ended(world: &World) -> bool {
        let entity = Self::entity_of(world, StableId::MATCH);
        let session = world.get_resource::<Session>();
        entity.is_some_and(|entity| world.get::<MatchEnd>(entity).is_some())
            || session.is_some_and(|session| session.log().result().is_some())
    }

    fn status(&self, world: &World) -> SessionStatus {
        let phase = match ServerPhase::now(world) {
            ServerPhase::Opening => SessionPhase::Opening,
            ServerPhase::Lobby => SessionPhase::Lobby,
            ServerPhase::Restoring => SessionPhase::Restoring,
            ServerPhase::Running => SessionPhase::Running,
            ServerPhase::Replaying => SessionPhase::Replaying,
        };
        let book = world.get_resource::<ResourceBook>();
        let entities = world.get_resource::<EntityIndex>().into_iter();
        let mut players: Vec<PlayerStatus> = entities
            .flat_map(EntityIndex::iter)
            .filter_map(|(_, entity)| {
                let player = world.get::<Player>(entity)?;
                let held = world.get::<PlayerResources>(entity);
                let resources = book.zip(held).map_or_else(BTreeMap::new, |(book, held)| {
                    book.0
                        .iter()
                        .map(|(resource, name)| (name.to_owned(), held.amount(resource)))
                        .collect()
                });
                Some(PlayerStatus {
                    player_slot: player.player_slot().get(),
                    resources,
                })
            })
            .collect();
        players.sort_unstable_by_key(|player| player.player_slot);
        SessionStatus {
            phase,
            rate: world
                .get_resource::<MatchRate>()
                .map(|rate| rate.hz().get()),
            state_hash: world
                .contains_resource::<StateRegistry>()
                .then(|| world.state_hash().to_string()),
            pace: self.pace.as_deref().map(Self::pace_status),
            players,
            script_failures: self.seen().failures().clone(),
        }
    }

    fn pace_status(pace: &Pace) -> PaceStatus {
        PaceStatus {
            paused: pace.is_paused(),
            speed: match pace.speed() {
                PaceSpeed::Half => GameSpeed::Half,
                PaceSpeed::Normal => GameSpeed::Normal,
                PaceSpeed::Double => GameSpeed::Double,
                PaceSpeed::Quadruple => GameSpeed::Quadruple,
            },
        }
    }

    /// Sets the pause and the speed `ask` names, the pace as it then stands; a refusal of a match
    /// that is not local, or while a run is under way, whose end sets the pace. Of the first
    /// tick's hold, a pause it names ends the hold, and a speed is the one the hold pauses at, so
    /// the hold's end undoes neither.
    fn pace(&self, run: &mut Option<ConsoleRun>, ask: PaceAsk) -> ConsoleAnswer {
        let Some(pace) = &self.pace else {
            return ConsoleAnswer::Refused(ConsoleRefusal::NotLocal);
        };
        if let Some(run) = run.as_ref().and_then(|run| run.id) {
            return ConsoleAnswer::Refused(ConsoleRefusal::RunUnderWay { run });
        }
        if let Some(paused) = ask.paused {
            pace.set_paused(paused);
        }
        if let Some(speed) = ask.speed {
            pace.set_speed(match speed {
                GameSpeed::Half => PaceSpeed::Half,
                GameSpeed::Normal => PaceSpeed::Normal,
                GameSpeed::Double => PaceSpeed::Double,
                GameSpeed::Quadruple => PaceSpeed::Quadruple,
            });
        }
        if ask.paused.is_some() {
            *run = None;
        } else if let Some(hold) = run {
            hold.before = pace.speed();
        }
        ConsoleAnswer::Paced(Self::pace_status(pace))
    }

    /// Starts the run `ask` of the request `id` into `run`: none to answer until it ends; an
    /// answer at once for a run that ends where it starts, and a refusal of a match that is not
    /// local, while another run is under way, or of a unit the match lacks.
    fn start(
        &self,
        world: &World,
        run: &mut Option<ConsoleRun>,
        id: RequestId,
        ask: &RunAsk,
    ) -> Option<ConsoleAnswer> {
        let Some(pace) = &self.pace else {
            return Some(ConsoleAnswer::Refused(ConsoleRefusal::NotLocal));
        };
        if let Some(run) = run.as_ref().and_then(|run| run.id) {
            return Some(ConsoleAnswer::Refused(ConsoleRefusal::RunUnderWay { run }));
        }
        let Some(next) = world
            .get_resource::<SimTick>()
            .map(|tick| tick.start().get())
        else {
            return Some(ConsoleAnswer::Refused(ConsoleRefusal::NoMatch));
        };
        let until = match self.until(world, next, &ask.to) {
            RunStart::Answered(answer) => return Some(answer),
            RunStart::Runs(until) => until,
        };
        let now = world.resource::<Time<Real>>().elapsed();
        *run = Some(ConsoleRun {
            id: Some(id),
            deadline: Some(now + ask.wall()),
            before: pace.start_run(),
            ..until
        });
        None
    }

    /// The run `to` names of a match whose next tick is `next`, or the answer of a run that ends
    /// where it starts, or of a unit the match lacks.
    fn until(&self, world: &World, next: u64, to: &RunTo) -> RunStart {
        let ended = Self::has_ended(world);
        let until = match to {
            &RunTo::Tick(tick) => RunUntil::Tick(tick),
            &RunTo::Ticks(ticks) => match ticks.checked_sub(1) {
                Some(more) => RunUntil::Tick(next + more),
                None => return RunStart::Answered(ConsoleAnswer::Ran(RunStop::Reached)),
            },
            &RunTo::Until(RunCondition::Unit { id, state }) => {
                let unit = StableId::from(StableNumber::new(id));
                if Self::entity_of(world, unit).is_none() {
                    return RunStart::Answered(ConsoleAnswer::Refused(
                        ConsoleRefusal::UnknownUnit { id },
                    ));
                }
                RunUntil::Unit(unit, Self::probe_state(state))
            }
            RunTo::Until(RunCondition::Event { message }) => {
                self.seen().watch(message.clone());
                RunUntil::Event
            }
            RunTo::Until(RunCondition::ScriptFailure) => {
                RunUntil::ScriptFailure(self.seen().failures().count)
            }
            RunTo::Until(RunCondition::MatchEnd) => RunUntil::MatchEnd,
        };
        let reached = matches!(until, RunUntil::Tick(tick) if next > tick);
        if reached || ended {
            let stop = if reached {
                RunStop::Reached
            } else {
                RunStop::MatchEnded
            };
            self.seen().unwatch();
            return RunStart::Answered(ConsoleAnswer::Ran(stop));
        }
        RunStart::Runs(ConsoleRun {
            id: None,
            until,
            deadline: None,
            before: PaceSpeed::Normal,
            cancelled: false,
        })
    }

    /// Why the run under way ends now, as `world` stands: its end reached, its match ended, a
    /// cancel, or its wall time past, in that order; none while it runs on, or with none under
    /// way.
    fn stop(&self, world: &World) -> Option<RunStop> {
        let queues = self.queues();
        let run = queues.run.as_ref()?;
        let ended = Self::has_ended(world);
        let held = match run.until {
            RunUntil::Tick(tick) => Self::ran(world).is_some_and(|ran| ran >= tick),
            RunUntil::Unit(id, state) => state.holds(world, id),
            RunUntil::Event => self.seen().is_watched_seen(),
            RunUntil::ScriptFailure(count) => self.seen().failures().count > count,
            RunUntil::MatchEnd => ended,
        };
        let now = world.resource::<Time<Real>>().elapsed();
        if held {
            Some(match run.until {
                RunUntil::Tick(_) => RunStop::Reached,
                _ => RunStop::Held,
            })
        } else if ended {
            Some(RunStop::MatchEnded)
        } else if run.cancelled {
            Some(RunStop::Cancelled)
        } else if run.deadline.is_some_and(|deadline| now >= deadline) {
            Some(RunStop::WallTime)
        } else {
            None
        }
    }

    const fn probe_state(state: UnitActivity) -> ProbeState {
        match state {
            UnitActivity::Idle => ProbeState::Idle,
            UnitActivity::Walking => ProbeState::Walking,
            UnitActivity::Stuck => ProbeState::Stuck,
            UnitActivity::Working => ProbeState::Working,
            UnitActivity::Fighting => ProbeState::Fighting,
            UnitActivity::Acting => ProbeState::Acting,
        }
    }

    /// The table `ask` asks of `world`'s units; a refusal before a match, or for a type the
    /// match has not, with the names nearest it.
    fn units(world: &World, ask: &UnitsAsk) -> ConsoleAnswer {
        if !world.contains_resource::<SimTick>() {
            return ConsoleAnswer::Refused(ConsoleRefusal::NoMatch);
        }
        let units = match &ask.units {
            UnitSelection::All => ProbeUnits::All,
            &UnitSelection::PlayerSlot(slot) => ProbeUnits::Owner(PlayerSlot::new(slot)),
            &UnitSelection::Team(team) => ProbeUnits::Team(Team::new(team)),
            UnitSelection::Type(name) => {
                let types = world
                    .get_resource::<UnitTypeBook>()
                    .map(UnitTypeBook::types);
                let found = types.and_then(|types| types.named((TypeScope::Mode, name.as_str())));
                let Some(unit_type) = found else {
                    let names = types.into_iter().flat_map(|types| {
                        let mode = |&(id, _): &_| types.scope(id) == TypeScope::Mode;
                        types.iter().filter(mode).map(|(_, name)| name)
                    });
                    return ConsoleAnswer::Refused(ConsoleRefusal::UnknownType {
                        name: name.clone(),
                        nearest: NearestNames::of(name, names),
                    });
                };
                ProbeUnits::Type(unit_type)
            }
            UnitSelection::Ids(ids) => ProbeUnits::Ids(
                ids.iter()
                    .map(|&id| StableId::from(StableNumber::new(id)))
                    .collect(),
            ),
            &UnitSelection::State(state) => ProbeUnits::State(Self::probe_state(state)),
        };
        let page = ProbePage {
            columns: match ask.detail {
                Detail::Concise => ProbeColumns::Concise,
                Detail::Detailed => ProbeColumns::Detailed,
            },
            first: usize::try_from(ask.first).expect("a u32 fits usize"),
            limit: ask
                .limit
                .map(|limit| usize::try_from(limit).expect("a u32 fits usize")),
        };
        let mut probe = MatchProbe::new(NonZeroU64::MIN, units);
        let rows = probe.page(world, page);
        let count = |rows| u32::try_from(rows).expect("a match's units count in a u32");
        ConsoleAnswer::Units(UnitTable {
            text: probe.text().to_owned(),
            selected: count(rows.selected),
            first: ask.first,
            shown: count(rows.shown),
        })
    }
}

#[cfg(test)]
mod tests;
