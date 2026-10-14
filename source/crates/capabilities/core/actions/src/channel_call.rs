use campfire_values::{ActionId, Rank};
use serde::{Deserialize, Serialize};

use crate::action_call::ActionCall;

/// What a tick of a channel did: the channel an order, a stop or an interrupt cut since the last
/// one, whose `on_interrupt` runs now, and the channel that ticked, whose `on_channel_tick` runs
/// now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelStep {
    pub interrupted: Option<ChannelCall>,
    pub ticked: Option<ChannelCall>,
}

/// A call of a channel: the call, and the action and rank its slot held as the channel ran, which
/// a cut channel keeps once an item leaves its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelCall {
    pub call: ActionCall,
    pub action: ActionId,
    pub rank: Rank,
}
