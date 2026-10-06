use std::mem;
use std::ops::Range;

use bevy_ecs::query::{QueryState, With};
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_capabilities::{Experience, Order, Owner};
use campfire_common::{PlayerSlot, Tick};
use campfire_log::LogEvent;
use campfire_protocol::{Controller, ServerInput, ServerInputError};
use campfire_runner::{ServerInputRefused, Session};
use campfire_sim::StableId;

use crate::events::avatar_missing::AvatarMissing;
use crate::events::input_dropped::InputDropped;
use crate::order_script::OrderScript;
use crate::server_bots::ServerBots;
use crate::server_signer::ServerSigner;
use crate::sim_client::bot_script::BotScript;

/// Plays the server's bots: before each tick, for each slot a bot plays, the orders and mode
/// inputs its script has for that tick, each a payload as a client writes it, which the server
/// signs and logs as a `Bot` input. A tick takes at most the session's max inputs a slot; the
/// rest wait for the next. A slot that becomes a bot's after its player left plays the takeover
/// script from the tick its leave applies in, as the log holds it, so a restore plays on where
/// the run before it stopped; one a player took is played no more, and a bot of the plan with no
/// script idles.
#[derive(Resource, Debug)]
pub(crate) struct BotDriver {
    bots: Vec<DrivenBot>,
    takeover: Option<OrderScript>,
    /// The payloads written and not logged yet, end to end, and each one's place, in order.
    payloads: Vec<u8>,
    waiting: Vec<Waiting>,
    /// The body of the order being written, kept so no order allocates.
    body: Vec<u8>,
    /// The slots whose tick takes no more inputs, and the payloads that wait for the next tick,
    /// kept so no tick allocates.
    full: Vec<PlayerSlot>,
    kept: Vec<Waiting>,
    /// The avatars, by their owners' slots, kept so no tick builds the query again.
    avatars: Avatars,
}

/// The query of the avatars and their owners.
type Avatars = QueryState<(&'static StableId, &'static Owner), With<Experience>>;

/// A bot the driver plays: its slot, its script, and the tick the script's ticks count from.
#[derive(Debug)]
struct DrivenBot {
    slot: PlayerSlot,
    script: BotScript,
    since: Tick,
}

/// A payload waiting for its slot's next tick.
#[derive(Debug, Clone)]
struct Waiting {
    slot: PlayerSlot,
    payload: Range<usize>,
}

impl BotDriver {
    /// The driver of `bots` in the match of `world`, from tick `next` on: what their scripts had
    /// for earlier ticks was logged already, by the run a restore follows.
    pub(crate) fn new(bots: ServerBots, next: Tick, world: &mut World) -> BotDriver {
        let ServerBots { slots, takeover } = bots;
        let bots = slots
            .into_iter()
            .map(|bot| DrivenBot::new(bot.slot, bot.script, Tick::new(0), next))
            .collect();
        BotDriver {
            bots,
            takeover,
            payloads: Vec::new(),
            waiting: Vec::new(),
            body: Vec::new(),
            full: Vec::new(),
            kept: Vec::new(),
            avatars: world.query_filtered(),
        }
    }

    /// Logs the bots' inputs for the next tick of the session in `world`.
    pub(crate) fn drive(world: &mut World) {
        world.resource_scope(|world, mut driver: Mut<'_, BotDriver>| {
            let tick = world.resource::<Session>().log().next_tick();
            driver.follow_controllers(world.resource::<Session>(), tick);
            driver.write_due(world, tick);
            world.resource_scope(|world, mut session: Mut<'_, Session>| {
                driver.serve(world.resource::<ServerSigner>(), &mut session);
            });
        });
    }

    /// Plays the takeover script in each slot that became a bot's after its player left, from the
    /// tick the leave applies in, as of `tick`, and stops playing each slot a player took.
    fn follow_controllers(&mut self, session: &Session, tick: Tick) {
        let log = session.log();
        let bot = |slot| log.controller(slot) == Some(Controller::Bot);
        self.bots.retain(|driven| bot(driven.slot));
        self.waiting.retain(|waiting| bot(waiting.slot));
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

    /// Writes the payloads of what each bot's script has due by `tick`: its mode inputs, then
    /// its orders, for the slot's avatar.
    fn write_due(&mut self, world: &World, tick: Tick) {
        for driven in &mut self.bots {
            let since = tick
                .since(driven.since)
                .expect("a bot plays from its start on");
            let due = driven.script.due(Tick::new(since.get()));
            for input in due.inputs {
                let start = self.payloads.len();
                input.write_payload(&mut self.payloads);
                self.waiting.push(Waiting {
                    slot: driven.slot,
                    payload: start..self.payloads.len(),
                });
            }
            let avatar = self
                .avatars
                .iter(world)
                .find(|(_, owner)| owner.slot() == driven.slot)
                .map(|(&id, _)| id);
            for scripted in due.orders {
                let Some(unit) = avatar else {
                    AvatarMissing {
                        slot: driven.slot,
                        action: format!("{:?}", scripted.action),
                    }
                    .log();
                    continue;
                };
                let order = Order {
                    unit,
                    action: scripted.action,
                };
                let start = self.payloads.len();
                order.write_payload(&mut self.body, &mut self.payloads);
                self.waiting.push(Waiting {
                    slot: driven.slot,
                    payload: start..self.payloads.len(),
                });
            }
        }
    }

    /// Logs the waiting payloads in order, each slot's until the tick takes no more of them; a
    /// payload past the session's max length is dropped.
    fn serve(&mut self, signer: &ServerSigner, session: &mut Session) {
        let (full, kept) = (&mut self.full, &mut self.kept);
        full.clear();
        kept.clear();
        for waiting in self.waiting.drain(..) {
            if full.contains(&waiting.slot) {
                kept.push(waiting);
                continue;
            }
            let input = ServerInput::Bot {
                slot: waiting.slot,
                payload: &self.payloads[waiting.payload.clone()],
            };
            match signer.serve(session, input) {
                Ok(()) => {}
                Err(ServerInputRefused::Log(ServerInputError::TooManyInputs)) => {
                    full.push(waiting.slot);
                    kept.push(waiting);
                }
                Err(ServerInputRefused::Log(ServerInputError::PayloadTooLarge)) => {
                    InputDropped {
                        name: format!("a bot's payload of {} bytes", waiting.payload.len()),
                    }
                    .log();
                }
                Err(error) => panic!("the server's bot input holds: {error}"),
            }
        }
        // Each kept payload moves to the front of the buffer, in order.
        let mut at = 0;
        for waiting in kept.iter_mut() {
            let len = waiting.payload.len();
            self.payloads.copy_within(waiting.payload.clone(), at);
            waiting.payload = at..at + len;
            at += len;
        }
        self.payloads.truncate(at);
        mem::swap(&mut self.waiting, &mut self.kept);
    }
}

impl DrivenBot {
    /// The bot of `slot` that plays `script` from tick `since`, from tick `next` on: what the
    /// script had for earlier ticks was logged already, by the run a restore follows.
    fn new(slot: PlayerSlot, script: OrderScript, since: Tick, next: Tick) -> DrivenBot {
        let mut script = BotScript::new(script);
        let played = next.since(since).expect("a bot plays from its start on");
        if let Some(before) = played.get().checked_sub(1) {
            script.due(Tick::new(before));
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
                    !bot.script.due(Tick::new(since.get())).orders.is_empty()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(ticks(10), [12, 16]);
        assert_eq!(ticks(15), [16]);
    }
}
