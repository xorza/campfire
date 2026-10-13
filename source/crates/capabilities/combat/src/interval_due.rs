use campfire_core::ModifierId;
use campfire_sim::StableId;

/// An instance whose interval comes this tick: its carrier, its modifier and its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct IntervalDue {
    pub(super) carrier: StableId,
    pub id: ModifierId,
    pub(super) source: Option<StableId>,
}
