use campfire_sim::StableId;

use crate::units::modifier_id::ModifierId;

/// An instance whose interval comes this tick: its carrier, its modifier and its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct IntervalDue {
    pub(super) carrier: StableId,
    pub(super) id: ModifierId,
    pub(super) source: Option<StableId>,
}
