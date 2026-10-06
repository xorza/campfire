use std::ops::Range;

use bevy_ecs::query::With;
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
/// script from then; one a player took is played no more.
#[derive(Resource, Debug)]
pub(crate) struct BotDriver {
    bots: Vec<DrivenBot>,
    takeover: Option<OrderScript>,
    /// The payloads written and not logged yet, end to end, and each one's place, in order.
    payloads: Vec<u8>,
    waiting: Vec<Waiting>,
    /// The body of the order being written, kept so no order allocates.
    body: Vec<u8>,
}

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
    /// The driver of `bots`, from tick `next` on: what their scripts had for earlier ticks was
    /// logged already, by the run a restore follows.
    pub(crate) fn new(bots: ServerBots, next: Tick) -> BotDriver {
        let ServerBots { slots, takeover } = bots;
        let bots = slots
            .into_iter()
            .map(|bot| {
                let mut script = BotScript::new(bot.script);
                if let Some(before) = next.get().checked_sub(1) {
                    script.due(Tick::new(before));
                }
                DrivenBot {
                    slot: bot.slot,
                    script,
                    since: Tick::new(0),
                }
            })
            .collect();
        BotDriver {
            bots,
            takeover,
            payloads: Vec::new(),
            waiting: Vec::new(),
            body: Vec::new(),
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

    /// Plays the takeover script in each slot that became a bot's, from `tick`, and stops
    /// playing each slot a player took.
    fn follow_controllers(&mut self, session: &Session, tick: Tick) {
        let log = session.log();
        let bot = |slot| log.controller(slot) == Some(Controller::Bot);
        self.bots.retain(|driven| bot(driven.slot));
        self.waiting.retain(|waiting| bot(waiting.slot));
        let Some(takeover) = &self.takeover else {
            return;
        };
        for slot in (0..log.slot_count()).map(PlayerSlot::new) {
            if bot(slot) && !self.bots.iter().any(|driven| driven.slot == slot) {
                self.bots.push(DrivenBot {
                    slot,
                    script: BotScript::new(takeover.clone()),
                    since: tick,
                });
            }
        }
    }

    /// Writes the payloads of what each bot's script has due by `tick`: its mode inputs, then
    /// its orders, for the slot's avatar.
    fn write_due(&mut self, world: &mut World, tick: Tick) {
        let mut avatars = world.query_filtered::<(&StableId, &Owner), With<Experience>>();
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
            let avatar = avatars
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
        let mut full = Vec::new();
        let mut kept = Vec::with_capacity(self.waiting.len());
        for waiting in self.waiting.drain(..) {
            if full.contains(&waiting.slot) {
                kept.push(waiting);
                continue;
            }
            let payload = self.payloads[waiting.payload.clone()].to_vec();
            let input = ServerInput::Bot {
                slot: waiting.slot,
                payload,
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
        for waiting in &mut kept {
            let len = waiting.payload.len();
            self.payloads.copy_within(waiting.payload.clone(), at);
            waiting.payload = at..at + len;
            at += len;
        }
        self.payloads.truncate(at);
        self.waiting = kept;
    }
}
