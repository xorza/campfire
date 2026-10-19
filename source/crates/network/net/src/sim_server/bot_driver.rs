use bevy_ecs::resource::Resource;
use bevy_ecs::system::ResMut;
use campfire_bots::{BotBuffers, BotScript, BotSink, BotView, Named, OrderUnsent};
use campfire_capabilities::Action;
use campfire_common::ErrorReport;
use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogEvent;
use campfire_protocol::{Controller, ServerInput, ServerInputError, SessionLog};
use campfire_runner::{ServerInputRefused, Session};
use campfire_vision::ByVision;

use crate::events::bot_order_unsent::BotOrderUnsent;
use crate::events::bot_payload_dropped::{BotPayloadDropped, DropCause};
use crate::sim_server::server_bots::{ServerBots, SlotBot};

/// Plays the server's bots: before each tick, for each slot a bot plays, the orders and mode
/// inputs its script has for that tick, each a payload as a client writes it, which the server
/// signs and logs as a `Bot` input at once; the log schedules each as a player's, so one past
/// the tick's max inputs waits in the log, and the journal and every checkpoint hold it. A slot
/// that becomes a bot's after its player left plays the takeover script from the tick its leave
/// applies in, as the log holds it, so a restore plays on where the run before it stopped; one a
/// player took is played no more, and a bot of the plan with no script idles.
#[derive(Resource, Debug)]
pub(crate) struct BotDriver {
    bots: Vec<DrivenBot>,
    takeover: Option<BotScript>,
    /// How many of the log's slot changes the driver followed: the log only adds them in a run.
    followed: usize,
    /// The buffers of the bots' steps, kept so no input allocates.
    buffers: BotBuffers,
}

/// A bot the driver plays: its slot, its script, and the tick the script's ticks count from.
#[derive(Debug)]
pub(super) struct DrivenBot {
    pub(super) slot: PlayerSlot,
    script: BotScript,
    since: Tick,
}

impl BotDriver {
    /// The driver of `bots` in the match of `log`, from tick `next` on: what their scripts had
    /// for earlier ticks was logged already, by the run a restore follows.
    pub(crate) fn new(bots: ServerBots, next: Tick, log: &SessionLog) -> Self {
        let ServerBots { slots, takeover } = bots;
        let bots = Self::planned(slots, log, next);
        Self {
            bots,
            takeover,
            followed: 0,
            buffers: BotBuffers::default(),
        }
    }

    /// The bots of the plan's `slots` that play their script from the start, from tick `next` of
    /// `log` on: a slot that changed hands since, as a restore finds one a player took, plays it
    /// no more, and gets the takeover script once its player left it to a bot.
    pub(super) fn planned(
        slots: impl IntoIterator<Item = SlotBot>,
        log: &SessionLog,
        next: Tick,
    ) -> Vec<DrivenBot> {
        slots
            .into_iter()
            .filter(|bot| log.changes().iter().all(|change| change.slot != bot.slot))
            .map(|bot| DrivenBot::new(bot.slot, bot.script, Tick::new(0), next))
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

    /// Plays the takeover script in each slot that became a bot's after its player left, from the
    /// tick the leave applies in, as of `tick`, and stops playing each slot a player took.
    fn follow_controllers(&mut self, session: &Session, tick: Tick) {
        let log = session.log();
        let bot = |slot| log.controller(slot) == Some(Controller::Bot);
        self.bots.retain(|driven| bot(driven.slot));
        let changes = &log.changes()[self.followed..];
        self.followed = log.changes().len();
        let Some(takeover) = &self.takeover else {
            return;
        };
        // The last change of a slot that is a bot's made it one: from the last, the first met of
        // each slot. A bot's slot with no change is the plan's, which has no script.
        for left in changes.iter().rev() {
            let slot = left.slot;
            if !bot(slot) || self.bots.iter().any(|driven| driven.slot == slot) {
                continue;
            }
            let script = takeover.clone();
            self.bots
                .push(DrivenBot::new(slot, script, left.tick, tick));
        }
    }

    /// Logs what each bot's script has due by `tick` into `session`, as the bots' one step gives
    /// it: its mode inputs, then its orders, each for the slot's units it names.
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
                .step(script_tick, driven.slot, view, buffers, &mut served);
        }
    }
}

/// The session the bots' inputs go into, whose signer signs each.
#[derive(Debug)]
struct BotServer<'s> {
    session: &'s mut Session,
}

impl BotSink for BotServer<'_> {
    /// Logs `payload` as an input of the bot of `slot`; one past the session's max length, past
    /// the ticks up to its max input lead, which the slot's inputs fill, or past a full log, is
    /// dropped.
    fn send(&mut self, slot: PlayerSlot, payload: &[u8]) {
        let input = ServerInput::Bot { slot, payload };
        let cause = match self.session.serve(input) {
            Ok(()) => return,
            Err(ServerInputRefused::Log(ServerInputError::PayloadTooLarge)) => DropCause::TooLarge,
            Err(ServerInputRefused::Log(ServerInputError::AheadOfTime)) => DropCause::AheadOfTime,
            Err(ServerInputRefused::Log(ServerInputError::LogFull)) => DropCause::LogFull,
            Err(error) => panic!("the server's bot input holds: {}", ErrorReport::of(&error)),
        };
        BotPayloadDropped {
            slot,
            len: payload.len(),
            cause,
        }
        .log();
    }

    fn unsent(&mut self, slot: PlayerSlot, action: &Action<Named>, cause: OrderUnsent) {
        BotOrderUnsent {
            slot,
            cause,
            action: action.clone(),
        }
        .log();
    }
}

impl DrivenBot {
    /// The bot of `slot` that plays `script` from tick `since`, from tick `next` on: what the
    /// script had for earlier ticks was logged already, by the run a restore follows.
    fn new(slot: PlayerSlot, mut script: BotScript, since: Tick, next: Tick) -> Self {
        let played = next.since(since).expect("a bot plays from its start on");
        if let Some(before) = played.get().checked_sub(1) {
            script.due_inputs(Tick::new(before));
            script.due_orders(Tick::new(before));
        }
        Self {
            slot,
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
            let mut bot =
                DrivenBot::new(PlayerSlot::new(1), script, Tick::new(10), Tick::new(next));
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
