use bevy_reflect::Reflect;
use campfire_net_sync::InputTimelineSetup;

use crate::rollback_mode::RollbackMode;

#[derive(Debug, Clone, Copy, Reflect)]
/// The RollbackConfig defines how we check and trigger rollbacks.
pub struct RollbackSetup {
    /// How a confirmed state that differs from the prediction triggers a rollback.
    pub mode: RollbackMode,
    /// Upper bound on the number of ticks we can roll back.
    ///
    /// A non-zero [`InputTimelineSetup::max_predicted_ticks`] can lower the effective bound.
    /// Rollback requests beyond the effective bound are ignored.
    pub max_rollback_ticks: u16,
}

impl Default for RollbackSetup {
    fn default() -> Self {
        Self {
            mode: RollbackMode::Check,
            max_rollback_ticks: 20,
        }
    }
}

impl RollbackSetup {
    /// Maximum rollback depth after applying the input timing configuration's prediction limit.
    ///
    /// `max_predicted_ticks` bounds how far ahead the local simulation can be, so rollback
    /// state older than that cannot be needed.
    pub fn effective_max_rollback_ticks(&self, input_setup: &InputTimelineSetup) -> u16 {
        self.max_rollback_ticks
            .min(input_setup.max_predicted_ticks())
    }
}
