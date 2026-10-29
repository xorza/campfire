use core::time::Duration;

use campfire_math::Num;

use crate::link_model::LinkModel;
use crate::phase_mode::PhaseMode;

/// From `start` on the wire clock, a direction follows `model` in `mode`, until the next phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase {
    pub start: Duration,
    pub model: LinkModel,
    pub mode: PhaseMode,
}

impl Phase {
    /// No packet arrives from `start`.
    pub fn outage(start: Duration) -> Self {
        Self {
            start,
            model: LinkModel::perfect().with_fixed_loss(Num::ONE),
            mode: PhaseMode::Pass,
        }
    }

    /// Packets sent from `start` follow `model` and wait for the next phase's start.
    pub const fn stall(start: Duration, model: LinkModel) -> Self {
        Self {
            start,
            model,
            mode: PhaseMode::Hold,
        }
    }

    /// From `start`, packets follow `model` again.
    pub const fn recovery(start: Duration, model: LinkModel) -> Self {
        Self {
            start,
            model,
            mode: PhaseMode::Pass,
        }
    }
}
