use campfire_common::Tick;
use campfire_values::{ActionId, ActionStart};
use serde::{Deserialize, Serialize};

use crate::cooldown_holder::CooldownHolder;
use crate::rank_values::RankValues;
use crate::slot_aim::SlotAim;

/// A started order: the tick it resolves in, and how it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartedOrder {
    pub resolves_at: Tick,
    pub start: ActionStart,
}

/// A call of a started action: the action and its target, and how the action started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCall {
    pub aim: SlotAim,
    pub start: ActionStart,
}

impl ActionCall {
    /// Whether its target lies within the world's bound and its start holds what one can, as a
    /// restore checks them.
    pub(crate) fn check(self) -> bool {
        self.aim.target.within_bound() && self.start.check()
    }
}

/// A cast that resolved: its action and who keeps its cooldown, its call, at the target its check gave, and its action's
/// values at its rank.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedCast {
    pub action: ActionId,
    pub holder: CooldownHolder,
    pub call: ActionCall,
    pub values: RankValues,
}
