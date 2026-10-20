use campfire_common::Tick;
use campfire_math::Vec3;
use campfire_sim::StableId;
use campfire_values::{ActionId, ActionStart, Rank};
use serde::{Deserialize, Serialize};

use crate::action_call::{ActionCall, StartedOrder};
use crate::channel_call::ChannelCall;
use crate::slot_aim::SlotAim;

/// What a unit has under way: an attack in its windup, or an action it was ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InProgress {
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
        origin: Vec3,
        since: Tick,
        full: Tick,
        released: bool,
    },
    /// The channel of the action `aim` names, which resolved at its target, as it `start`ed: it
    /// ticks next in `next`, and ends in `ends`; `action` at `rank` is what its slot held as it
    /// began, which a cut channel keeps.
    Channel {
        aim: SlotAim,
        next: Tick,
        ends: Tick,
        start: ActionStart,
        action: ActionId,
        rank: Rank,
    },
}

/// How far an ordered action got: not checked yet, walking in range of its target, which then
/// owns its unit's walk, or started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderPhase {
    Ordered,
    Approaching,
    Started(StartedOrder),
}

impl InProgress {
    /// Whether each of its places lies within the world's bound and its start holds what one
    /// can, as a restore checks them.
    pub(crate) fn check(self) -> bool {
        match self {
            Self::Attack { .. } => true,
            Self::Order { aim, phase } => {
                let started = match phase {
                    OrderPhase::Started(started) => started.start.check(),
                    OrderPhase::Ordered | OrderPhase::Approaching => true,
                };
                aim.target.within_bound() && started
            }
            Self::Charge { aim, origin, .. } => aim.target.within_bound() && origin.within_bound(),
            Self::Channel { aim, start, .. } => aim.target.within_bound() && start.check(),
        }
    }

    /// The slot whose action is under way.
    pub const fn slot(self) -> u8 {
        let mut copy = self;
        *copy.slot_mut()
    }

    /// The slot, to move when slots are added before it.
    pub(super) const fn slot_mut(&mut self) -> &mut u8 {
        match self {
            Self::Attack { slot, .. }
            | Self::Order {
                aim: SlotAim { slot, .. },
                ..
            }
            | Self::Charge {
                aim: SlotAim { slot, .. },
                ..
            }
            | Self::Channel {
                aim: SlotAim { slot, .. },
                ..
            } => slot,
        }
    }

    /// The call of a channel, at its action and rank; none of anything else.
    pub const fn channel_call(self) -> Option<ChannelCall> {
        match self {
            Self::Channel {
                aim,
                start,
                action,
                rank,
                ..
            } => Some(ChannelCall {
                call: ActionCall { aim, start },
                action,
                rank,
            }),
            Self::Attack { .. } | Self::Order { .. } | Self::Charge { .. } => None,
        }
    }

    /// The call of a started order that resolves by `now`: of a cast, as only a cast starts.
    pub fn cast_due(self, now: Tick) -> Option<ActionCall> {
        match self {
            Self::Order {
                aim,
                phase: OrderPhase::Started(StartedOrder { resolves_at, start }),
            } if resolves_at <= now => Some(ActionCall { aim, start }),
            _ => None,
        }
    }

    /// The tick it resolves in, once started.
    pub const fn resolves_at(self) -> Option<Tick> {
        match self {
            Self::Attack { resolves_at, .. }
            | Self::Order {
                phase: OrderPhase::Started(StartedOrder { resolves_at, .. }),
                ..
            } => Some(resolves_at),
            Self::Order {
                phase: OrderPhase::Ordered | OrderPhase::Approaching,
                ..
            }
            | Self::Charge { .. }
            | Self::Channel { .. } => None,
        }
    }
}
