use thiserror::Error;

/// A byte that names no seat plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("the byte {0} names no seat plan")]
pub struct NotSeatPlan(pub u8);
