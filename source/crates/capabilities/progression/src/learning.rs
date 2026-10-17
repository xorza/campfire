use bevy_ecs::system::{Res, SystemParam};
use campfire_actions::{Action, ActionBook, ActionSlot, ChargeRule, SlotKindBook};
use campfire_stats::Level;
use campfire_values::Rank;

use crate::points::Points;

/// The rule of the `learn` order, as `progression` checks it and a client's HUD shows it: how many
/// ranks a slot's action has, and whether a unit may learn its next one; and, for the HUD, the
/// charges its action holds.
#[derive(SystemParam, Debug)]
pub struct Learning<'w> {
    book: Res<'w, ActionBook>,
    kinds: Res<'w, SlotKindBook>,
}

impl Learning<'_> {
    /// How many ranks the action in `held` has, when its kind's ranks are learned; none for a
    /// kind whose one rank comes with the spawn.
    pub fn ranks(&self, held: ActionSlot) -> Option<u8> {
        if self.kinds.0.first_rank(held.kind).is_some() {
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
        let next = held.rank.map_or(Some(Rank::FIRST), Rank::next);
        let next = next.filter(|&next| action.has_rank(next));
        points.get() > 0
            && next.is_some_and(|next| {
                self.kinds
                    .0
                    .level_of(held.kind, next)
                    .is_none_or(|needed| needed.get() <= level.get())
            })
    }
    /// The charges of the action in `held` at its rank: how many it holds at most, and the ticks
    /// one takes to come back; none for an action without charges, or one not learned.
    pub fn charges(&self, held: ActionSlot) -> Option<ChargeRule> {
        self.action(held)?.values_at(held.rank?)?.charges
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
