use std::sync::Arc;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_bots::{
    BotBuffers, BotNames, BotScript, BotSink, HeldBotView, OrderUnsent, PacedBot, Think,
};
use campfire_capabilities::{Books, Directive, NamedRefs};
use campfire_common::{ErrorReport, Seat, Tick};
use campfire_declarations::MatchFacts;
use campfire_log::LogEvent;
use campfire_protocol::{Controller, ServerInput, ServerInputError, SessionLog};
use campfire_runner::{ServerInputRefusal, Session};
use campfire_script::Budget;
use campfire_sim::MatchRate;

use crate::events::bot_order_unsent::BotOrderUnsent;
use crate::events::bot_payload_dropped::{BotPayloadDropped, DropCause};
use crate::events::bot_think_failed::BotThinkFailed;
use crate::sim_server::server_bots::{BotPlay, SeatBot, ServerBots};

/// Plays the server's bots: before each tick, for each seat a bot plays, the orders and mode
/// inputs its order script has for that tick, or the orders a bot of the mode gives in a think,
/// each a payload as a client writes it, which the server signs and logs as a `Bot` input at
/// once; the log schedules each as a player's, so one past the tick's max inputs waits in the
/// log, and the journal and every checkpoint hold it. A seat that becomes a bot's after its
/// player left plays the takeover script from the tick its leave applies in, as the log holds
/// it, so a restore plays on where the run before it stopped; one a player took is played no
/// more, and a bot of the plan with nothing to play idles.
#[derive(Resource, Debug)]
pub(crate) struct BotDriver {
    bots: Vec<DrivenBot>,
    takeover: Option<BotScript>,
    /// How many of the log's seat changes the driver followed: the log only adds them in a run.
    followed: usize,
    /// The buffers of the bots' steps, kept so no input allocates.
    buffers: BotBuffers,
    /// The match's books, in which a think's orders resolve their names.
    books: Arc<Books>,
    view: HeldBotView,
    /// The operations the thinks of one tick run together, the mode's `think` limit, as a
    /// tick's AI thinks do: a think that finds them spent waits for the next tick
    /// (03-game-scripting.md, The thinks of a tick share a pool).
    pool: u64,
}

/// A bot the driver plays: its seat, and what plays it.
#[derive(Debug)]
pub(super) struct DrivenBot {
    pub(super) seat: Seat,
    plays: Plays,
}

/// What plays a driven bot: an order script, from the tick its ticks count from; or a bot of the
/// mode, at its pace.
#[derive(Debug)]
enum Plays {
    Orders { script: BotScript, since: Tick },
    Thinks(PacedBot),
}

impl BotDriver {
    /// The driver of `bots` in the session of `world`, from tick `next` on: what their scripts
    /// had for earlier ticks was logged already, by the run a restore follows.
    pub(crate) fn new(bots: ServerBots, next: Tick, world: &mut World) -> Self {
        let ServerBots {
            seats,
            takeover,
            books,
        } = bots;
        let rate = *world.resource::<MatchRate>();
        let pools = world.resource::<MatchFacts>().role.pools();
        let pool = pools.expect("a server's match runs scripts").limits.think;
        let bots = Self::planned(seats, world.resource::<Session>().log(), next, rate);
        Self {
            bots,
            takeover,
            followed: 0,
            buffers: BotBuffers::default(),
            books,
            view: HeldBotView::new(world),
            pool,
        }
    }

    /// The bots of the plan's `seats` that play from the start, from tick `next` of `log` on, in
    /// a match at `rate`: a seat that changed hands since, as a restore finds one a player took,
    /// plays it no more, and gets the takeover script once its player left it to a bot.
    pub(super) fn planned(
        seats: impl IntoIterator<Item = SeatBot>,
        log: &SessionLog,
        next: Tick,
        rate: MatchRate,
    ) -> Vec<DrivenBot> {
        seats
            .into_iter()
            .filter(|bot| log.changes().iter().all(|change| change.seat != bot.seat))
            .map(|SeatBot { seat, play }| {
                let plays = match play {
                    BotPlay::Orders(script) => Plays::orders(script, Tick::new(0), next),
                    BotPlay::Thinks(bot) => Plays::Thinks(PacedBot::new(bot, rate)),
                };
                DrivenBot { seat, plays }
            })
            .collect()
    }

    /// Logs the bots' inputs for the next tick of the session.
    pub(crate) fn drive(world: &mut World) {
        world.resource_scope(|world, mut driver: Mut<'_, Self>| {
            world.resource_scope(|world, mut session: Mut<'_, Session>| {
                let tick = session.log().next_tick();
                driver.follow_controllers(&session, tick);
                driver.log_due(world, &mut session, tick);
            });
        });
    }

    /// Plays the takeover script in each seat that became a bot's after its player left, from the
    /// tick the leave applies in, as of `tick`, and stops playing each seat a player took.
    fn follow_controllers(&mut self, session: &Session, tick: Tick) {
        let log = session.log();
        let bot = |seat| log.controller(seat) == Some(Controller::Bot);
        self.bots.retain(|driven| bot(driven.seat));
        let changes = &log.changes()[self.followed..];
        self.followed = log.changes().len();
        let Some(takeover) = &self.takeover else {
            return;
        };
        // The last change of a seat that is a bot's made it one: from the last, the first met of
        // each seat. A bot's seat with no change is the plan's, which has no script.
        for left in changes.iter().rev() {
            let seat = left.seat;
            if !bot(seat) || self.bots.iter().any(|driven| driven.seat == seat) {
                continue;
            }
            let plays = Plays::orders(takeover.clone(), left.tick, tick);
            self.bots.push(DrivenBot { seat, plays });
        }
    }

    /// Logs into `session` what each bot has due by `tick` in the match of `world`, in seat
    /// order of the plan, then of the takeovers: an order script's mode inputs, then its orders,
    /// as the bots' one step gives them; a bot's orders of its think, when it thinks.
    fn log_due(&mut self, world: &mut World, session: &mut Session, tick: Tick) {
        let Self {
            bots,
            buffers,
            books,
            view,
            pool,
            ..
        } = self;
        let names = BotNames::new(books);
        let mut served = BotServer { session };
        let mut budget = Budget::new(*pool);
        for driven in bots {
            let seat = driven.seat;
            match &mut driven.plays {
                Plays::Orders { script, since } => {
                    let since = tick.since(*since).expect("a bot plays from its start on");
                    let view = view.get(world);
                    script.step(Tick::new(since.get()), seat, &view, buffers, &mut served);
                }
                Plays::Thinks(bot) => {
                    let think = Think {
                        seat,
                        now: tick,
                        budget: &mut budget,
                        names: &names,
                        view,
                        buffers,
                        sink: &mut served,
                    };
                    if let Err(error) = bot.play(world, think) {
                        BotThinkFailed {
                            seat,
                            error: ErrorReport::of(&error).to_string(),
                        }
                        .log();
                    }
                }
            }
        }
    }
}

/// The session the bots' inputs go into, whose signer signs each.
#[derive(Debug)]
struct BotServer<'s> {
    session: &'s mut Session,
}

impl BotSink for BotServer<'_> {
    /// Logs `payload` as an input of the bot of `seat`; one past the session's max length, past
    /// the ticks up to its max input lead, which the seat's inputs fill, or past a full log, is
    /// dropped.
    fn send(&mut self, seat: Seat, payload: &[u8]) {
        let input = ServerInput::Bot { seat, payload };
        let cause = match self.session.serve(input) {
            Ok(()) => return,
            Err(ServerInputRefusal::Log(ServerInputError::PayloadTooLarge)) => DropCause::TooLarge,
            Err(ServerInputRefusal::Log(ServerInputError::AheadOfTime)) => DropCause::AheadOfTime,
            Err(ServerInputRefusal::Log(ServerInputError::LogFull)) => DropCause::LogFull,
            Err(error) => panic!("the server's bot input holds: {}", ErrorReport::of(&error)),
        };
        BotPayloadDropped {
            seat,
            len: payload.len(),
            cause,
        }
        .log();
    }

    fn unsent(&mut self, seat: Seat, directive: &Directive<NamedRefs>, cause: OrderUnsent) {
        BotOrderUnsent {
            seat,
            cause,
            directive: directive.clone(),
        }
        .log();
    }
}

impl Plays {
    /// The order script `script` from tick `since`, from tick `next` on: what the script had for
    /// earlier ticks was logged already, by the run a restore follows.
    fn orders(mut script: BotScript, since: Tick, next: Tick) -> Self {
        let played = next.since(since).expect("a bot plays from its start on");
        if let Some(before) = played.get().checked_sub(1) {
            script.due_inputs(Tick::new(before));
            script.due_orders(Tick::new(before));
        }
        Self::Orders { script, since }
    }
}

#[cfg(test)]
mod tests {
    use campfire_bots::OrderScript;
    use campfire_test_packages::TestMode;

    use super::*;

    #[test]
    fn a_bot_built_later_skips_what_its_script_had_for_the_ticks_before() {
        // Orders at script ticks 2 and 6 of a bot from tick 10. Built at tick 10, it plays both;
        // built at tick 15, as a restore does, script ticks 0 to 4 were logged, so it plays the
        // second only, at tick 16.
        let script = OrderScript::parse(
            "[[order]]\ntick = 2\ndirective.orders.move.to = [1, 0]\n[[order]]\ntick = 6\ndirective.orders.move.to = [2, 0]\n",
        )
        .unwrap();
        let books = TestMode::Lane.packages().name_books();
        let script = BotScript::new(&script, &BotNames::new(&books)).unwrap();
        let ticks = |next: u64| {
            let played = Plays::orders(script.clone(), Tick::new(10), Tick::new(next));
            let Plays::Orders { mut script, since } = played else {
                unreachable!("an order script plays orders");
            };
            (next..20)
                .filter(|&tick| {
                    let since = Tick::new(tick).since(since).unwrap();
                    !script.due_orders(Tick::new(since.get())).is_empty()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(ticks(10), [12, 16]);
        assert_eq!(ticks(15), [16]);
    }
}
