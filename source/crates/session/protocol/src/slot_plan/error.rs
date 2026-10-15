use thiserror::Error;

/// A byte that names no slot plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("the byte {0} names no slot plan")]
pub struct NotSlotPlan(pub u8);
