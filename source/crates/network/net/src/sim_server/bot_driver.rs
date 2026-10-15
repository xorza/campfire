use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_capabilities::Order;
use campfire_common::{PlayerSlot, Tick};
use campfire_log::{ErrorReport, LogEvent};
use campfire_protocol::{Controller, ServerInput, ServerInputError, SessionLog};
use campfire_runner::{ServerInputRefused, Session};
use campfire_units::HeldPlayerUnits;

use crate::bot_script::BotScript;
use crate::events::avatar_missing::AvatarMissing;
use crate::events::bot_payload_dropped::{BotPayloadDropped, DropCause};
use crate::order_script::OrderScript;
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
    takeover: Option<OrderScript>,
    /// The payload being logged, and the body of the command in it, kept so no order allocates.
    payload: Vec<u8>,
    body: Vec<u8>,
    /// Who each slot commands, kept so no tick builds its query again.
    players: HeldPlayerUnits,
}

/// A bot the driver plays: its slot, its script, and the tick the script's ticks count from.
#[derive(Debug)]
pub(super) struct DrivenBot {
    pub(super) slot: PlayerSlot,
    script: BotScript,
    since: Tick,
}

impl BotDriver {
    /// The driver of `bots` in the match of `world`, from tick `next` on: what their scripts had
    /// for earlier ticks was logged already, by the run a restore follows.
    pub(crate) fn new(bots: ServerBots, next: Tick, world: &mut World) -> BotDriver {
        let ServerBots { slots, takeover } = bots;
        let bots = BotDriver::planned(slots, world.resource::<Session>().log(), next);
        BotDriver {
            bots,
            takeover,
            payload: Vec::new(),
            body: Vec::new(),
            players: HeldPlayerUnits::new(world),
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

    /// Logs the bots' inputs for the next tick of the session in `world`.
    pub(crate) fn drive(world: &mut World) {
        world.resource_scope(|world, mut driver: Mut<'_, BotDriver>| {
            let tick = world.resource::<Session>().log().next_tick();
            driver.follow_controllers(world.resource::<Session>(), tick);
            world.resource_scope(|world, mut session: Mut<'_, Session>| {
                driver.log_due(world, &mut session, tick);
            });
        });
    }

    /// Plays the takeover script in each slot that became a bot's after its player left, from the
    /// tick the leave applies in, as of `tick`, and stops playing each slot a player took.
    fn follow_controllers(&mut self, session: &Session, tick: Tick) {
        let log = session.log();
        let bot = |slot| log.controller(slot) == Some(Controller::Bot);
        self.bots.retain(|driven| bot(driven.slot));
        let Some(takeover) = &self.takeover else {
            return;
        };
        for slot in (0..log.slot_count()).map(PlayerSlot::new) {
            if !bot(slot) || self.bots.iter().any(|driven| driven.slot == slot) {
                continue;
            }
            // A bot's slot with no change is the plan's, which has no script.
            let Some(left) = log.changes().iter().rfind(|change| change.slot == slot) else {
                continue;
            };
            let script = takeover.clone();
            self.bots
                .push(DrivenBot::new(slot, script, left.tick, tick));
        }
    }

    /// Logs what each bot's script has due by `tick` into `session`: its mode inputs, then its
    /// orders, for the slot's avatar.
    fn log_due(&mut self, world: &World, session: &mut Session, tick: Tick) {
        let BotDriver {
            bots,
            payload,
            body,
            players,
            ..
        } = self;
        for driven in bots {
            let since = tick
                .since(driven.since)
                .expect("a bot plays from its start on");
            let script_tick = Tick::new(since.get());
            for input in driven.script.due_inputs(script_tick) {
                payload.clear();
                input.write_payload(body, payload);
                BotDriver::log(session, driven.slot, payload);
            }
            let avatar = players.avatar(world, driven.slot).map(|avatar| avatar.id);
            for scripted in driven.script.due_orders(script_tick) {
                let Some(unit) = avatar else {
                    AvatarMissing {
                        slot: driven.slot,
                        action: scripted.action,
                    }
                    .log();
                    continue;
                };
                payload.clear();
                Order::one(unit, scripted.action).write_payload(body, payload);
                BotDriver::log(session, driven.slot, payload);
            }
        }
    }

    /// Logs `payload` as an input of the bot of `slot`; one past the session's max length, or
    /// past the ticks up to its max input lead, which the slot's inputs fill, is dropped.
    fn log(session: &mut Session, slot: PlayerSlot, payload: &[u8]) {
        let cause = match session.serve(ServerInput::Bot { slot, payload }) {
            Ok(()) => return,
            Err(ServerInputRefused::Log(ServerInputError::PayloadTooLarge)) => DropCause::TooLarge,
            Err(ServerInputRefused::Log(ServerInputError::AheadOfTime)) => DropCause::AheadOfTime,
            Err(error) => panic!("the server's bot input holds: {}", ErrorReport::of(&error)),
        };
        BotPayloadDropped {
            slot,
            len: payload.len(),
            cause,
        }
        .log();
    }
}

impl DrivenBot {
    /// The bot of `slot` that plays `script` from tick `since`, from tick `next` on: what the
    /// script had for earlier ticks was logged already, by the run a restore follows.
    fn new(slot: PlayerSlot, script: OrderScript, since: Tick, next: Tick) -> DrivenBot {
        let mut script = BotScript::new(script);
        let played = next.since(since).expect("a bot plays from its start on");
        if let Some(before) = played.get().checked_sub(1) {
            script.due_inputs(Tick::new(before));
            script.due_orders(Tick::new(before));
        }
        DrivenBot {
            slot,
            script,
            since,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bot_built_later_skips_what_its_script_had_for_the_ticks_before() {
        // Orders at script ticks 2 and 6 of a bot from tick 10. Built at tick 10, it plays both;
        // built at tick 15, as a restore does, script ticks 0 to 4 were logged, so it plays the
        // second only, at tick 16.
        let script = OrderScript::parse(
            "[[order]]\ntick = 2\nmove = [1, 0]\n[[order]]\ntick = 6\nmove = [2, 0]\n",
        )
        .unwrap();
        let ticks = |next: u64| {
            let mut bot = DrivenBot::new(
                PlayerSlot::new(1),
                script.clone(),
                Tick::new(10),
                Tick::new(next),
            );
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
