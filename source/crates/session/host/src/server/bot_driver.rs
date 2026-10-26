use std::sync::Arc;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_bots::{
    BotNames, BotScript, BotSink, OrderUnsent, PacedBot, PlayerSlotPlay, ThinkError, TickBot,
    TickBots,
};
use campfire_capabilities::{Books, Directive, NamedRefs};
use campfire_common::{ErrorReport, PlayerSlot, Tick};
use campfire_declarations::MatchFacts;
use campfire_log::LogEvent;
use campfire_protocol::{Controller, SessionLog};
use campfire_runner::Session;
use campfire_sim::MatchRate;

use crate::events::bot_order_unsent::BotOrderUnsent;
use crate::events::bot_payload_dropped::BotPayloadDropped;
use crate::events::bot_think_failed::BotThinkFailed;
use crate::server::server_bots::{BotPlay, PlayerSlotBot, ServerBots};

/// Plays the server's bots: before each tick, for each player slot a bot plays, the orders and mode
/// inputs its order script has for that tick, or the orders a bot of the mode gives in a think,
/// each a payload as a client writes it, which the server signs and logs as a `Bot` input at
/// once; the log schedules each as a player's, so one past the tick's max inputs waits in the
/// log, and the journal and every checkpoint hold it. A player slot that becomes a bot's after its
/// player left plays the takeover script from the tick its leave applies in, as the log holds
/// it, so a restore plays on where the run before it stopped; one a player took is played no
/// more, and a bot of the plan with nothing to play idles.
#[derive(Resource, Debug)]
pub struct BotDriver {
    /// The server's bots, which a load plans again from.
    plan: ServerBots,
    bots: TickBots,
    takeover: Option<BotScript>,
    /// How many of the log's player slot changes the driver followed: the log only adds them in a
    /// run.
    followed: usize,
    /// The match's books, in which a think's orders resolve their names.
    books: Arc<Books>,
}

impl BotDriver {
    /// The driver of `bots` in the session of `world`, from tick `next` on: what their scripts
    /// had for earlier ticks was logged already, by the run a restore follows.
    pub fn new(bots: ServerBots, next: Tick, world: &mut World) -> Self {
        let plan = bots.clone();
        let ServerBots {
            player_slots,
            takeover,
            books,
        } = bots;
        let rate = *world.resource::<MatchRate>();
        let setup = world.resource::<MatchFacts>().role.budgets();
        let think_limit = setup.expect("a server's match runs scripts").limits.think;
        let planned = Self::planned(player_slots, world.resource::<Session>().log(), next, rate);
        let mut bots = TickBots::new(world, think_limit);
        for bot in planned {
            bots.push(bot);
        }
        Self {
            plan,
            bots,
            takeover,
            followed: 0,
            books,
        }
    }

    /// Plans the driver of `world` again from its server's bots, as the session went back to a
    /// save in place: from tick `next`, the save's, as a restore's driver starts.
    pub fn load(world: &mut World, next: Tick) {
        let plan = world.resource::<Self>().plan.clone();
        let driver = Self::new(plan, next, world);
        world.insert_resource(driver);
    }

    /// The bots of the plan's `player_slots` that play from the start, from tick `next` of `log`
    /// on, in a match at `rate`: a player slot that changed hands since, as a restore finds one a
    /// player took, plays it no more, and gets the takeover script once its player left it to a
    /// bot.
    pub fn planned(
        player_slots: impl IntoIterator<Item = PlayerSlotBot>,
        log: &SessionLog,
        next: Tick,
        rate: MatchRate,
    ) -> Vec<TickBot> {
        player_slots
            .into_iter()
            .filter(|bot| {
                log.changes()
                    .iter()
                    .all(|change| change.player_slot != bot.player_slot)
            })
            .map(|PlayerSlotBot { player_slot, play }| {
                let play = match play {
                    BotPlay::Orders(script) => PlayerSlotPlay::orders(script, Tick::new(0), next),
                    BotPlay::Thinks(bot) => PlayerSlotPlay::Thinks(PacedBot::new(bot, rate)),
                };
                TickBot { player_slot, play }
            })
            .collect()
    }

    /// Logs the bots' inputs for the next tick of the session.
    pub fn drive(world: &mut World) {
        world.resource_scope(|world, mut driver: Mut<'_, Self>| {
            world.resource_scope(|world, mut session: Mut<'_, Session>| {
                let tick = session.log().next_tick();
                driver.follow_controllers(&session, tick);
                driver.log_due(world, &mut session, tick);
            });
        });
    }

    /// Plays the takeover script in each player slot that became a bot's after its player left,
    /// from the tick the leave applies in, as of `tick`, and stops playing each player slot a
    /// player took.
    fn follow_controllers(&mut self, session: &Session, tick: Tick) {
        let log = session.log();
        let bot = |player_slot| log.controller(player_slot) == Some(Controller::Bot);
        self.bots.retain(|driven| bot(driven.player_slot));
        let changes = &log.changes()[self.followed..];
        self.followed = log.changes().len();
        let Some(takeover) = &self.takeover else {
            return;
        };
        // The last change of a player slot that is a bot's made it one: from the last, the first
        // met of each player slot. A bot's player slot with no change is the plan's, which has no
        // script.
        for left in changes.iter().rev() {
            let player_slot = left.player_slot;
            if !bot(player_slot)
                || self
                    .bots
                    .iter()
                    .any(|driven| driven.player_slot == player_slot)
            {
                continue;
            }
            let play = PlayerSlotPlay::orders(takeover.clone(), left.tick, tick);
            self.bots.push(TickBot { player_slot, play });
        }
    }

    /// Logs into `session` what each bot has due by `tick` in the match of `world`, as the bots
    /// of a tick play: an order script's mode inputs, then its orders; a bot's orders of its
    /// think, when it thinks.
    fn log_due(&mut self, world: &mut World, session: &mut Session, tick: Tick) {
        let names = BotNames::new(&self.books);
        let _ = self
            .bots
            .play(world, tick, &names, &mut BotServer { session });
    }
}

/// The session the bots' inputs go into, whose signer signs each.
#[derive(Debug)]
struct BotServer<'s> {
    session: &'s mut Session,
}

impl BotSink for BotServer<'_> {
    /// The longest payload the session's terms take of an input.
    fn max_payload_len(&self) -> u32 {
        self.session.log().header().terms.inputs.max_payload_len
    }

    /// Logs `payload` as an input of the bot of `player_slot`; one past the session's max length,
    /// past the ticks up to its max input lead, which the player slot's inputs fill, or past a full
    /// log, is dropped.
    fn send(&mut self, player_slot: PlayerSlot, payload: &[u8]) {
        let Err(cause) = self.session.serve_bot(player_slot, payload) else {
            return;
        };
        BotPayloadDropped {
            player_slot,
            len: payload.len(),
            cause,
        }
        .log();
    }

    fn unsent(
        &mut self,
        player_slot: PlayerSlot,
        directive: &Directive<NamedRefs>,
        cause: OrderUnsent,
    ) {
        BotOrderUnsent {
            player_slot,
            cause,
            directive: directive.clone(),
        }
        .log();
    }

    fn failed(&mut self, player_slot: PlayerSlot, error: ThinkError) {
        BotThinkFailed {
            player_slot,
            error: ErrorReport::of(&error).to_string(),
        }
        .log();
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
        let packages = TestMode::Lane.packages();
        let script = BotScript::new(&script, &BotNames::new(packages.load_books())).unwrap();
        let ticks = |next: u64| {
            let played = PlayerSlotPlay::orders(script.clone(), Tick::new(10), Tick::new(next));
            let PlayerSlotPlay::Orders { mut script, since } = played else {
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
