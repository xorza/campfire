use std::marker::PhantomData;

use bevy_ecs::system::{Local, Res, SystemParam};
use campfire_common::PlayerSlot;
use campfire_declarations::MatchFacts;
use campfire_sim::{StableId, TickInputs};
use campfire_units::Owner;
use campfire_visibility::OutView;

use crate::capability_directive::CapabilityDirective;
use crate::out_of_play::OutOfPlay;
use crate::player_rows::PlayerRows;

/// The orders of one capability among the tick's inputs, of directive `A`, each decoded as it is
/// read, in input order, into buffers each tick reuses. A body that is no such order is left out,
/// and so is each order of a player out of play in a session whose players out of play see what a
/// spectator sees: it would act on what its team does not see (10-replication-audiences.md, A
/// player out of play sees what its team sees, or what a spectator sees).
#[derive(SystemParam, Debug)]
pub struct TickCommands<'w, 's, A: CapabilityDirective> {
    inputs: Res<'w, TickInputs>,
    facts: Res<'w, MatchFacts>,
    out: PlayerRows<'w, 's, OutOfPlay>,
    /// The units of the order read last.
    units: Local<'s, Vec<StableId>>,
    /// The orders each player sent this tick so far, for the next one's number.
    sent: Local<'s, Vec<Sent>>,
    directive: PhantomData<A>,
}

/// An order of the tick: the player that sent it, its place among that player's orders of its
/// capability in the tick, which a client that predicts its own orders numbers alike, its directive
/// and its units.
#[derive(Debug, Clone, Copy)]
pub struct TickCommand<'a, A> {
    pub player_slot: PlayerSlot,
    pub number: u32,
    pub directive: A,
    pub units: &'a [StableId],
}

#[derive(Debug, Clone, Copy)]
struct Sent {
    player_slot: PlayerSlot,
    orders: u32,
}

impl<A: CapabilityDirective> TickCommands<'_, '_, A> {
    /// Gives `apply` each order of the tick, in input order.
    pub fn for_each(&mut self, mut apply: impl FnMut(TickCommand<'_, A>)) {
        let spectating = self.facts.visibility.out() == OutView::Spectators;
        self.sent.clear();
        for command in self.inputs.commands(A::CAPABILITY) {
            if spectating && self.out.of_player_slot(command.player_slot).is_some() {
                continue;
            }
            self.units.clear();
            let Some(directive) = A::decode_into(command.body, &mut self.units) else {
                continue;
            };
            apply(TickCommand {
                player_slot: command.player_slot,
                number: number(&mut self.sent, command.player_slot),
                directive,
                units: &self.units,
            });
        }
    }
}

impl<A> TickCommand<'_, A> {
    /// Whether its player controls a unit of `owner`, which alone carries it out.
    pub fn controls(&self, owner: Owner) -> bool {
        owner.player_slot() == self.player_slot
    }
}

/// The number of `player_slot`'s next order, counted in `sent`.
fn number(sent: &mut Vec<Sent>, player_slot: PlayerSlot) -> u32 {
    let at = sent.iter().position(|sent| sent.player_slot == player_slot);
    let at = at.unwrap_or_else(|| {
        sent.push(Sent {
            player_slot,
            orders: 0,
        });
        sent.len() - 1
    });
    let number = sent[at].orders;
    sent[at].orders += 1;
    number
}
