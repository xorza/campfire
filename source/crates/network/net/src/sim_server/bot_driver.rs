use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_common::{PlayerSlot, Tick};
use campfire_log::{ErrorReport, LogEvent};
use campfire_protocol::{Controller, ServerInput, ServerInputError, SessionLog, Signer};
use campfire_runner::{ServerInputRefused, Session};
use campfire_sim::StableId;
use campfire_units::{HeldPlayerUnits, UnitTypeBook};

use crate::bot_script::BotScript;
use crate::events::bot_payload_dropped::{BotPayloadDropped, DropCause};
use crate::events::bot_units_missing::BotUnitsMissing;
use crate::order_script::OrderScript;
use crate::sim_server::server_bots::{ServerBots, SlotBot};
use crate::sim_server::server_signer::ServerSigner;

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
    /// The payload being logged, and the body of the command in it, kept so no order allocates.
    payload: Vec<u8>,
    body: Vec<u8>,
    /// Who each slot commands, kept so no tick builds its query again.
    players: HeldPlayerUnits,
    /// The units of an order of a unit type, kept so no order allocates their list.
    scratch: Vec<StableId>,
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
    /// for earlier ticks was logged already, by the run a restore follows. Each unit type a
    /// script names is one of the match's, as the server's start checked.
    pub(crate) fn new(bots: ServerBots, next: Tick, world: &mut World) -> Self {
        let ServerBots { slots, takeover } = bots;
        let types = world.resource::<UnitTypeBook>().types();
        let script = |script: &OrderScript| {
            BotScript::new(script, types).expect("the server's start checked its bots' unit types")
        };
        let takeover = takeover.as_ref().map(script);
        let log = world.resource::<Session>().log();
        let bots = Self::planned(slots, log, next, script);
        Self {
            bots,
            takeover,
            followed: 0,
            payload: Vec::new(),
            body: Vec::new(),
            players: HeldPlayerUnits::new(world),
            scratch: Vec::new(),
        }
    }

    /// The bots of the plan's `slots` that play their script from the start, from tick `next` of
    /// `log` on: a slot that changed hands since, as a restore finds one a player took, plays it
    /// no more, and gets the takeover script once its player left it to a bot.
    pub(super) fn planned(
        slots: impl IntoIterator<Item = SlotBot>,
        log: &SessionLog,
        next: Tick,
        script: impl Fn(&OrderScript) -> BotScript,
    ) -> Vec<DrivenBot> {
        slots
            .into_iter()
            .filter(|bot| log.changes().iter().all(|change| change.slot != bot.slot))
            .map(|bot| DrivenBot::new(bot.slot, script(&bot.script), Tick::new(0), next))
            .collect()
    }

    /// Logs the bots' inputs for the next tick of the session in `world`.
    pub(crate) fn drive(world: &mut World) {
        world.resource_scope(|world, mut driver: Mut<'_, Self>| {
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

    /// Logs what each bot's script has due by `tick` into `session`: its mode inputs, then its
    /// orders, each for the slot's units it names.
    fn log_due(&mut self, world: &World, session: &mut Session, tick: Tick) {
        let Self {
            bots,
            payload,
            body,
            players,
            scratch,
            ..
        } = self;
        let mut served = BotServer {
            session,
            signer: &world.resource::<ServerSigner>().0,
        };
        for driven in bots {
            let since = tick
                .since(driven.since)
                .expect("a bot plays from its start on");
            let script_tick = Tick::new(since.get());
            for input in driven.script.due_inputs(script_tick) {
                payload.clear();
                input.write_payload(body, payload);
                served.log(driven.slot, payload);
            }
            let units = players.get(world);
            for scripted in driven.script.due_orders(script_tick) {
                let order = match scripted.order(driven.slot, &units, scratch) {
                    Ok(order) => order,
                    Err(cause) => {
                        BotUnitsMissing {
                            slot: driven.slot,
                            cause,
                            action: scripted.action,
                        }
                        .log();
                        continue;
                    }
                };
                payload.clear();
                order.write_payload(body, payload);
                served.log(driven.slot, payload);
            }
        }
    }
}

/// The session the bots' inputs go into, with the server's signer, which signs each.
#[derive(Debug)]
struct BotServer<'s> {
    session: &'s mut Session,
    signer: &'s Signer,
}

impl BotServer<'_> {
    /// Logs `payload` as an input of the bot of `slot`; one past the session's max length, past
    /// the ticks up to its max input lead, which the slot's inputs fill, or past a full log, is
    /// dropped.
    fn log(&mut self, slot: PlayerSlot, payload: &[u8]) {
        let input = ServerInput::Bot { slot, payload };
        let cause = match self.session.serve(input, self.signer) {
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
    use campfire_values::Names;

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
            let script = BotScript::new(&script, &Names::default()).unwrap();
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
