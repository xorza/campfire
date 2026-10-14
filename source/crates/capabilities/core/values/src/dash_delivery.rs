use campfire_math::Num;
use campfire_sim::{Position, StableId};
use serde::{Deserialize, Serialize};

use crate::action_id::ActionId;
use crate::action_start::ActionStart;
use crate::rank::Rank;

/// A dash that delivers the instant action `action` of `source` at `rank`, started as `start`
/// says, as that action's `on_resolve` started it: its `on_end` runs as the dash ends. With it,
/// the meters the dash went so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashDelivery {
    pub source: StableId,
    pub action: ActionId,
    pub rank: Rank,
    pub start: Option<ActionStart>,
    pub dashed: Num,
}

impl DashDelivery {
    /// The delivery of `action` of `source` at `rank`, started as `start` says, before the dash
    /// went anywhere.
    pub const fn new(
        source: StableId,
        action: ActionId,
        rank: Rank,
        start: Option<ActionStart>,
    ) -> DashDelivery {
        DashDelivery {
            source,
            action,
            rank,
            start,
            dashed: Num::ZERO,
        }
    }

    /// Adds the way of a step from `from` to `to`. A dash goes at most a step a tick, so only a
    /// match far past any session's length reaches the end of a number, where it stays.
    pub fn went(&mut self, from: Position, to: Position) {
        self.dashed = self
            .dashed
            .checked_add(from.get().distance(to.get()))
            .unwrap_or(Num::MAX);
    }
}
