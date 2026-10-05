use bevy_ecs::system::{Res, SystemParam};

use crate::actions::action::Action;
use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlot;
use crate::actions::slot_kinds::SlotKinds;
use crate::progression::points::Points;
use crate::stats::level::Level;

/// The rule of the `learn` order, as the orders check it and a client's HUD shows it: how many
/// ranks a slot's action has, and whether a unit may learn its next one.
#[derive(SystemParam, Debug)]
pub struct Learning<'w> {
    book: Res<'w, ActionBook>,
    kinds: Res<'w, SlotKinds>,
}

impl Learning<'_> {
    /// How many ranks the action in `held` has, when its kind's ranks are learned; none for a
    /// kind whose one rank comes with the spawn.
    pub fn ranks(&self, held: ActionSlot) -> Option<u8> {
        if self.kinds.first_rank(held.kind) != 0 {
            return None;
        }
        let ranks = self.action(held)?.ranks.len();
        Some(u8::try_from(ranks).expect("an action has few ranks"))
    }

    /// Whether a unit with `points` at `level` may learn the next rank of the action in `held`: it
    /// has a point, the action a rank above, and the unit the level its kind gives that rank.
    pub fn learnable(&self, held: ActionSlot, points: Points, level: Level) -> bool {
        let Some(action) = self.action(held) else {
            return false;
        };
        let next = held
            .rank
            .checked_add(1)
            .filter(|&next| action.has_rank(next));
        points.get() > 0
            && next.is_some_and(|next| {
                self.kinds
                    .level_of(held.kind, next)
                    .is_none_or(|needed| needed <= level)
            })
    }
    /// The action in `held`; none in an inventory slot that holds no action.
    fn action(&self, held: ActionSlot) -> Option<&Action> {
        let action = held.action?;
        Some(
            self.book
                .get(action)
                .expect("a slot's action is in the book"),
        )
    }
}
