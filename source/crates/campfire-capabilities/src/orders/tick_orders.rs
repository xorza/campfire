use std::ops::Range;

use bevy_ecs::resource::Resource;
use bevy_ecs::system::ResMut;
use campfire_common::PlayerSlot;
use campfire_sim::{StableId, TickInputs};

use crate::declare::scratch::Scratch;
use crate::orders::order::{Action, Order};

/// The orders of the tick's inputs, decoded once as the tick starts, in input order; a body that
/// is no order is left out. Not state: each tick reads its own into the same buffers.
#[derive(Resource, Debug, Default)]
pub(crate) struct TickOrders {
    orders: Vec<Stored>,
    /// The units of every order, each order's a run of them.
    units: Vec<StableId>,
    /// The orders each player sent this tick so far, for the next one's number.
    sent: Vec<Sent>,
}

#[derive(Debug, Clone, Copy)]
struct Sent {
    slot: PlayerSlot,
    orders: u32,
}

#[derive(Debug, Clone)]
struct Stored {
    slot: PlayerSlot,
    number: u32,
    action: Action,
    units: Range<u32>,
}

/// An order of the tick: the player that sent it, its place among that player's orders of the
/// tick, which a client that predicts its own orders numbers alike, its action and its units.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TickOrder<'a> {
    pub(crate) slot: PlayerSlot,
    pub(crate) number: u32,
    pub(crate) action: Action,
    pub(crate) units: &'a [StableId],
}

impl TickOrders {
    /// Empties the orders once the tick's inputs ran, which read them all, keeping the buffers.
    pub(crate) fn empty(mut orders: ResMut<'_, TickOrders>) {
        orders.orders.clear();
        orders.units.clear();
        orders.sent.clear();
    }

    /// Reads the orders of `inputs`, but those of each player `refused` names, into the buffers
    /// the last tick emptied.
    pub(crate) fn read(&mut self, inputs: &TickInputs, refused: impl Fn(PlayerSlot) -> bool) {
        debug_assert!(self.is_clear(), "the last tick emptied its orders");
        for command in inputs.commands(Order::CAPABILITY) {
            if refused(command.slot) {
                continue;
            }
            let start = self.units.len();
            let Some(action) = Order::decode_into(command.body, &mut self.units) else {
                continue;
            };
            let at = self.sent.iter().position(|sent| sent.slot == command.slot);
            let at = at.unwrap_or_else(|| {
                self.sent.push(Sent {
                    slot: command.slot,
                    orders: 0,
                });
                self.sent.len() - 1
            });
            let number = self.sent[at].orders;
            self.sent[at].orders += 1;
            let run = |at: usize| u32::try_from(at).expect("a tick's payloads fit a u32");
            self.orders.push(Stored {
                slot: command.slot,
                number,
                action,
                units: run(start)..run(self.units.len()),
            });
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = TickOrder<'_>> {
        self.orders.iter().map(|stored| TickOrder {
            slot: stored.slot,
            number: stored.number,
            action: stored.action,
            units: &self.units[stored.units.start as usize..stored.units.end as usize],
        })
    }
}

impl Scratch for TickOrders {
    fn is_clear(&self) -> bool {
        self.orders.is_empty() && self.units.is_empty() && self.sent.is_empty()
    }
}
