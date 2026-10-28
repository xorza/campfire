use bevy_ecs::system::{Query, Res};
use campfire_actions::ActionSlots;
use campfire_players::{OrderedUnits, TickCommands};
use campfire_sim::{EntityIndex, StableId};
use campfire_stats::{Level, PlayerModifiers};

use crate::learning::Learning;
use crate::perk_book::{Learner, PerkBook};
use crate::player_experience::PlayerExperience;
use crate::player_perks::PlayerPerks;
use crate::player_points::PlayerPoints;
use crate::points::Points;
use crate::progression_directive::ProgressionDirective;

/// The tick's orders that learn the next rank of a slot's action, or a perk.
#[derive(Debug)]
pub struct LearnOrders;

/// The parts of a player a learn of a perk reads and changes.
type PlayerLearns = (
    &'static mut PlayerPerks,
    Option<&'static mut PlayerPoints>,
    Option<&'static PlayerExperience>,
    &'static mut PlayerModifiers,
);

impl LearnOrders {
    /// Applies each learn order of the tick, in input order, to each of its units by stable id, so
    /// a second learn in a tick sees the point the first spent: to a unit its player controls, dead
    /// or not, the next rank of the action in the slot, for a point, when the unit's level reaches
    /// the one the slot's kind gives that rank. It changes nothing under way. An order that fails a
    /// check is dropped: a client can send anything.
    pub(super) fn learn_ranks(
        mut orders: TickCommands<'_, '_, ProgressionDirective>,
        learning: Learning<'_>,
        mut units: OrderedUnits<'_, '_, (&mut ActionSlots, &mut Points, &Level)>,
    ) {
        orders.for_each(|order| {
            let ProgressionDirective::Learn { slot } = order.directive else {
                return;
            };
            units.for_each_mut(&order, |_, _, (mut slots, mut points, &level)| {
                let learnable = slots
                    .slot(slot)
                    .is_some_and(|held| learning.learnable(held, *points, level));
                if learnable {
                    points.spend();
                    slots.learn(slot);
                }
            });
        });
    }

    /// Applies each perk's learn order of the tick, in input order, so a second learn in a tick
    /// sees the points the first spent: the perk, for its cost, to the order's player, which then
    /// holds its modifier, when the player has its points, its level and the perks it requires, and
    /// lacks it (04-capabilities/progression.md, A science is a perk of a player). An order of a
    /// perk no player learns, or that fails a check, is dropped: a client can send anything. The
    /// server's, as no client predicts its player's state.
    pub(super) fn learn_perks(
        mut orders: TickCommands<'_, '_, ProgressionDirective>,
        book: Res<'_, PerkBook>,
        index: Res<'_, EntityIndex>,
        mut players: Query<'_, '_, PlayerLearns>,
    ) {
        orders.for_each(|order| {
            let ProgressionDirective::LearnPerk { perk } = order.directive else {
                return;
            };
            if !book.names().has(perk) {
                return;
            }
            let Some(entity) = index.get(StableId::of_player(order.player_slot)) else {
                return;
            };
            let (mut perks, mut points, experience, mut modifiers) = players
                .get_mut(entity)
                .expect("a player of a mode of perks holds its perks and modifiers");
            let held = Learner {
                perks: perks.get(),
                points: points.as_deref().map(|points| points.get()),
                experience: experience.map(PlayerExperience::get),
            };
            let Some(cost) = book.learn_cost(perk, held) else {
                return;
            };
            points
                .as_mut()
                .expect("a perk of a cost is learned for points")
                .spend(cost);
            perks.add(perk);
            modifiers.add(book.modifier(perk));
        });
    }
}
