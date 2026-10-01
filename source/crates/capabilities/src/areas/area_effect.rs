use campfire_sim::Position;

use crate::deliveries::delivering::Delivering;

/// An area a call queued: of `by`, at `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AreaEffect {
    pub(crate) by: Delivering,
    pub(crate) at: Position,
}
