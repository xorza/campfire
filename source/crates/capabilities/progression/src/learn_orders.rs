use campfire_actions::ActionSlots;
use campfire_players::{OrderedUnits, TickCommands};
use campfire_stats::Level;

use crate::learning::Learning;
use crate::points::Points;
use crate::progression_directive::ProgressionDirective;

/// The tick's orders that learn the next rank of a slot's action.
#[derive(Debug)]
pub struct LearnOrders;

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
            let ProgressionDirective::Learn { slot } = order.directive;
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
}
