use bevy_ecs::system::{Query, Res};
use campfire_sim::EntityIndex;

use crate::actions::action_slots::ActionSlots;
use crate::players::tick_commands::TickCommands;
use crate::progression::learning::Learning;
use crate::progression::points::Points;
use crate::progression::progression_action::ProgressionAction;
use crate::stats::level::Level;
use crate::units::owner::Owner;

/// The tick's orders that learn the next rank of a slot's action.
#[derive(Debug)]
pub(super) struct LearnOrders;

impl LearnOrders {
    /// Applies each learn order of the tick, in input order, to each of its units by stable id, so
    /// a second learn in a tick sees the point the first spent: to a unit its player controls, dead
    /// or not, the next rank of the action in the slot, for a point, when the unit's level reaches
    /// the one the slot's kind gives that rank. It changes nothing under way. An order that fails a
    /// check is dropped: a client can send anything.
    pub(super) fn learn_ranks(
        mut orders: TickCommands<'_, '_, ProgressionAction>,
        index: Res<'_, EntityIndex>,
        learning: Learning<'_>,
        mut units: Query<'_, '_, (&Owner, &mut ActionSlots, &mut Points, &Level)>,
    ) {
        orders.for_each(|order| {
            let ProgressionAction::Learn { slot } = order.action;
            for &unit in order.units {
                let Some(Ok((owner, mut slots, mut points, &level))) =
                    index.get(unit).map(|entity| units.get_mut(entity))
                else {
                    continue;
                };
                let learnable = slots
                    .slot(slot)
                    .is_some_and(|held| learning.learnable(held, *points, level));
                if !order.controls(*owner) || !learnable {
                    continue;
                }
                points.spend();
                slots.learn(slot);
            }
        });
    }
}
