use campfire_common::Tick;
use campfire_sim::{Position, StableId};
use serde::{Deserialize, Serialize};

use crate::actions::action_call::{ActionCall, Started};
use crate::actions::slot_aim::SlotAim;
use crate::values::action_start::ActionStart;

/// What a unit has under way: an attack in its windup, or an action it was ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum InProgress {
    /// The weapon in `slot`, started at `target` from the attack target, to resolve in
    /// `resolves_at`: an attack is under way only once started.
    Attack {
        slot: u8,
        target: StableId,
        resolves_at: Tick,
    },
    /// The action `aim` names, as ordered, in its `phase`. Whether it casts or trains is its
    /// action's kind, and only a cast walks in range and starts.
    Order { aim: SlotAim, phase: OrderPhase },
    /// The charged action `aim` names, charging from `since` at the target its check kept, from
    /// `origin`: full in `full`, and `released` once its order came again. It resolves when
    /// released, or full.
    Charge {
        aim: SlotAim,
        origin: Position,
        since: Tick,
        full: Tick,
        released: bool,
    },
    /// The channel of the action `aim` names, which resolved at its target, as it `start`ed: it
    /// ticks next in `next`, and ends in `ends`.
    Channel {
        aim: SlotAim,
        next: Tick,
        ends: Tick,
        start: ActionStart,
    },
}

/// How far an ordered action got: not checked yet, walking in range of its target, which then
/// owns its unit's walk, or started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum OrderPhase {
    Ordered,
    Approaching,
    Started(Started),
}

impl InProgress {
    /// The slot whose action is under way.
    pub(crate) const fn slot(self) -> u8 {
        let mut copy = self;
        *copy.slot_mut()
    }

    /// The slot, to move when slots are added before it.
    pub(super) const fn slot_mut(&mut self) -> &mut u8 {
        match self {
            InProgress::Attack { slot, .. }
            | InProgress::Order {
                aim: SlotAim { slot, .. },
                ..
            }
            | InProgress::Charge {
                aim: SlotAim { slot, .. },
                ..
            }
            | InProgress::Channel {
                aim: SlotAim { slot, .. },
                ..
            } => slot,
        }
    }

    /// The call of a started order that resolves by `now`: of a cast, as only a cast starts.
    pub(crate) fn cast_due(self, now: Tick) -> Option<ActionCall> {
        match self {
            InProgress::Order {
                aim,
                phase: OrderPhase::Started(Started { resolves_at, start }),
            } if resolves_at <= now => Some(ActionCall { aim, start }),
            _ => None,
        }
    }

    /// The tick it resolves in, once started.
    pub(crate) const fn resolves_at(self) -> Option<Tick> {
        match self {
            InProgress::Attack { resolves_at, .. }
            | InProgress::Order {
                phase: OrderPhase::Started(Started { resolves_at, .. }),
                ..
            } => Some(resolves_at),
            InProgress::Order {
                phase: OrderPhase::Ordered | OrderPhase::Approaching,
                ..
            }
            | InProgress::Charge { .. }
            | InProgress::Channel { .. } => None,
        }
    }
}
