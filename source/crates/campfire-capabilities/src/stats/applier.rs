use campfire_sim::StableId;

use crate::stats::lifetime::Hold;
use crate::units::action_id::ActionId;
use crate::values::rank::Rank;

/// Who applies a modifier: its source, none from the mode; the ability that applies it, at
/// `rank`, rank 1 with none; and what holds it, a passive or an aura, an area or a player, or
/// none for an application of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Applier {
    pub(crate) source: Option<StableId>,
    pub(crate) ability: Option<ActionId>,
    pub(crate) rank: Rank,
    pub(crate) hold: Option<Hold>,
}
