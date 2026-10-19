use bevy_ecs::resource::Resource;
use bevy_ecs::system::ResMut;
use campfire_bots::{BotBuffers, BotScript, BotSink, BotView, Named, OrderUnsent};
use campfire_capabilities::Action;
use campfire_common::ErrorReport;
use campfire_common::{Seat, Tick};
use campfire_log::LogEvent;
use campfire_protocol::{Controller, ServerInput, ServerInputError, SessionLog};
use campfire_runner::{ServerInputRefusal, Session};
use campfire_vision::ByVision;

use crate::events::bot_order_unsent::BotOrderUnsent;
use crate::events::bot_payload_dropped::{BotPayloadDropped, DropCause};
use crate::sim_server::server_bots::{SeatBot, ServerBots};

/// Plays the server's bots: before each tick, for each seat a bot plays, the orders and mode
/// inputs its script has for that tick, each a payload as a client writes it, which the server
/// signs and logs as a `Bot` input at once; the log schedules each as a player's, so one past
/// the tick's max inputs waits in the log, and the journal and every checkpoint hold it. A seat
/// that becomes a bot's after its player left plays the takeover script from the tick its leave
/// applies in, as the log holds it, so a restore plays on where the run before it stopped; one a
/// player took is played no more, and a bot of the plan with no script idles.
#[derive(Resource, Debug)]
pub(crate) struct BotDriver {
    bots: Vec<DrivenBot>,
    takeover: Option<BotScript>,
    /// How many of the log's seat changes the driver followed: the log only adds them in a run.
    followed: usize,
    /// The buffers of the bots' steps, kept so no input allocates.
    buffers: BotBuffers,
}

/// A bot the driver plays: its seat, its script, and the tick the script's ticks count from.
#[derive(Debug)]
pub(super) struct DrivenBot {
    pub(super) seat: Seat,
    script: BotScript,
    since: Tick,
}

impl BotDriver {
    /// The driver of `bots` in the match of `log`, from tick `next` on: what their scripts had
    /// for earlier ticks was logged already, by the run a restore follows.
    pub(crate) fn new(bots: ServerBots, next: Tick, log: &SessionLog) -> Self {
        let ServerBots { seats, takeover } = bots;
        let bots = Self::planned(seats, log, next);
        Self {
            bots,
            takeover,
            followed: 0,
            buffers: BotBuffers::default(),
        }
    }

    /// The bots of the plan's `seats` that play their script from the start, from tick `next` of
    /// `log` on: a seat that changed hands since, as a restore finds one a player took, plays it
    /// no more, and gets the takeover script once its player left it to a bot.
    pub(super) fn planned(
        seats: impl IntoIterator<Item = SeatBot>,
        log: &SessionLog,
        next: Tick,
    ) -> Vec<DrivenBot> {
        seats
            .into_iter()
            .filter(|bot| log.changes().iter().all(|change| change.seat != bot.seat))
            .map(|bot| DrivenBot::new(bot.seat, bot.script, Tick::new(0), next))
            .collect()
    }

    /// Logs the bots' inputs for the next tick of the session.
    pub(crate) fn drive(
        mut driver: ResMut<'_, Self>,
        mut session: ResMut<'_, Session>,
        view: BotView<'_, '_, ByVision>,
    ) {
        let tick = session.log().next_tick();
        driver.follow_controllers(&session, tick);
        driver.log_due(&view, &mut session, tick);
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
            let script = takeover.clone();
            self.bots
                .push(DrivenBot::new(seat, script, left.tick, tick));
        }
    }

    /// Logs what each bot's script has due by `tick` into `session`, as the bots' one step gives
    /// it: its mode inputs, then its orders, each for the seat's units it names.
    fn log_due(&mut self, view: &BotView<'_, '_, ByVision>, session: &mut Session, tick: Tick) {
        let Self { bots, buffers, .. } = self;
        let mut served = BotServer { session };
        for driven in bots {
            let since = tick
                .since(driven.since)
                .expect("a bot plays from its start on");
            let script_tick = Tick::new(since.get());
            driven
                .script
                .step(script_tick, driven.seat, view, buffers, &mut served);
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

    fn unsent(&mut self, seat: Seat, action: &Action<Named>, cause: OrderUnsent) {
        BotOrderUnsent {
            seat,
            cause,
            action: action.clone(),
        }
        .log();
    }
}

impl DrivenBot {
    /// The bot of `seat` that plays `script` from tick `since`, from tick `next` on: what the
    /// script had for earlier ticks was logged already, by the run a restore follows.
    fn new(seat: Seat, mut script: BotScript, since: Tick, next: Tick) -> Self {
        let played = next.since(since).expect("a bot plays from its start on");
        if let Some(before) = played.get().checked_sub(1) {
            script.due_inputs(Tick::new(before));
            script.due_orders(Tick::new(before));
        }
        Self {
            seat,
            script,
            since,
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_bots::{BotNames, OrderScript};
    use campfire_test_packages::TestMode;

    use super::*;

    #[test]
    fn a_bot_built_later_skips_what_its_script_had_for_the_ticks_before() {
        // Orders at script ticks 2 and 6 of a bot from tick 10. Built at tick 10, it plays both;
        // built at tick 15, as a restore does, script ticks 0 to 4 were logged, so it plays the
        // second only, at tick 16.
        let script = OrderScript::parse(
            "[[order]]\ntick = 2\naction.orders.move.to = [1, 0]\n[[order]]\ntick = 6\naction.orders.move.to = [2, 0]\n",
        )
        .unwrap();
        let books = TestMode::Lane.packages().name_books();
        let script = BotScript::new(&script, &BotNames::new(&books)).unwrap();
        let ticks = |next: u64| {
            let script = script.clone();
            let mut bot = DrivenBot::new(Seat::new(1), script, Tick::new(10), Tick::new(next));
            (next..20)
                .filter(|&tick| {
                    let since = Tick::new(tick).since(bot.since).unwrap();
                    !bot.script.due_orders(Tick::new(since.get())).is_empty()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(ticks(10), [12, 16]);
        assert_eq!(ticks(15), [16]);
    }
}
