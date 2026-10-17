use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

use crate::action_id::ActionId;
use crate::action_start::ActionStart;
use crate::rank::Rank;

/// A dash that delivers the instant action `action` of `source` at `rank`, started as `start`
/// says, as that action's `on_resolve` started it: its `on_end` runs as the dash ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashDelivery {
    pub source: StableId,
    pub action: ActionId,
    pub rank: Rank,
    pub start: Option<ActionStart>,
}
