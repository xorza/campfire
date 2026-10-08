use serde::{Deserialize, Serialize};

use crate::actions::action_call::ActionCall;
use crate::units::action_id::ActionId;
use crate::values::rank::Rank;

/// What a tick of a channel did: the channel an order, a stop or an interrupt cut since the last
/// one, whose `on_interrupt` runs now, and the channel that ticked, whose `on_channel_tick` runs
/// now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChannelStep {
    pub(crate) interrupted: Option<ChannelCall>,
    pub(crate) ticked: Option<ChannelCall>,
}

/// A call of a channel: the call, and the action and rank its slot held as the channel ran, which
/// a cut channel keeps once an item leaves its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChannelCall {
    pub(crate) call: ActionCall,
    pub(crate) action: ActionId,
    pub(crate) rank: Rank,
}
