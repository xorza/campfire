use campfire_common::Tick;
use serde::{Deserialize, Serialize};

use crate::actions::rank_values::RankValues;
use crate::actions::slot_aim::SlotAim;
use crate::values::action_start::ActionStart;

/// A started order: the tick it resolves in, and how it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Started {
    pub(crate) resolves_at: Tick,
    pub(crate) start: ActionStart,
}

/// A call of a started action: the action and its target, and how the action started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ActionCall {
    pub(crate) aim: SlotAim,
    pub(crate) start: ActionStart,
}

/// A cast that resolved: its call, at the target its check gave, and its action's values at its
/// rank.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedCast {
    pub(crate) call: ActionCall,
    pub(crate) values: RankValues,
}
