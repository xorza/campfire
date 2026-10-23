use thiserror::Error;

/// A byte that names no player slot plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("the byte {0} names no player slot plan")]
pub struct NotPlayerSlotPlan(pub u8);
