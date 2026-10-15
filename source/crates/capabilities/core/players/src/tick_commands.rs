use std::marker::PhantomData;

use bevy_ecs::system::{Local, Res, SystemParam};
use campfire_common::PlayerSlot;
use campfire_sim::{StableId, TickInputs};
use campfire_units::Owner;
use campfire_visibility::OutView;

use crate::capability_action::CapabilityAction;
use crate::out_of_play::OutOfPlay;
use crate::player_rows::PlayerRows;

/// The orders of one capability among the tick's inputs, of action `A`, each decoded as it is
/// read, in input order, into buffers each tick reuses. A body that is no such order is left
/// out, and so is each order of a player out of play in a session whose players out of play see
/// what a spectator sees: it would act on what its team does not see (design 14, D16).
#[derive(SystemParam, Debug)]
pub struct TickCommands<'w, 's, A: CapabilityAction> {
    inputs: Res<'w, TickInputs>,
    out_view: Res<'w, OutView>,
    out: PlayerRows<'w, 's, OutOfPlay>,
    /// The units of the order read last.
    units: Local<'s, Vec<StableId>>,
    /// The orders each player sent this tick so far, for the next one's number.
    sent: Local<'s, Vec<Sent>>,
    action: PhantomData<A>,
}

/// An order of the tick: the player that sent it, its place among that player's orders of its
/// capability in the tick, which a client that predicts its own orders numbers alike, its action
/// and its units.
#[derive(Debug, Clone, Copy)]
pub struct TickCommand<'a, A> {
    pub slot: PlayerSlot,
    pub number: u32,
    pub action: A,
    pub units: &'a [StableId],
}

#[derive(Debug, Clone, Copy)]
struct Sent {
    slot: PlayerSlot,
    orders: u32,
}

impl<A: CapabilityAction> TickCommands<'_, '_, A> {
    /// Gives `apply` each order of the tick, in input order.
    pub fn for_each(&mut self, mut apply: impl FnMut(TickCommand<'_, A>)) {
        let spectating = *self.out_view == OutView::Spectators;
        self.sent.clear();
        for command in self.inputs.commands(A::CAPABILITY) {
            if spectating && self.out.of_slot(command.slot).is_some() {
                continue;
            }
            self.units.clear();
            let Some(action) = A::decode_into(command.body, &mut self.units) else {
                continue;
            };
            apply(TickCommand {
                slot: command.slot,
                number: number(&mut self.sent, command.slot),
                action,
                units: &self.units,
            });
        }
    }
}

impl<A> TickCommand<'_, A> {
    /// Whether its player controls a unit of `owner`, which alone carries it out.
    pub fn controls(&self, owner: Owner) -> bool {
        owner.slot() == self.slot
    }
}

/// The number of `slot`'s next order, counted in `sent`.
fn number(sent: &mut Vec<Sent>, slot: PlayerSlot) -> u32 {
    let at = sent.iter().position(|sent| sent.slot == slot);
    let at = at.unwrap_or_else(|| {
        sent.push(Sent { slot, orders: 0 });
        sent.len() - 1
    });
    let number = sent[at].orders;
    sent[at].orders += 1;
    number
}
