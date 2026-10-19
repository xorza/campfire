//! Defines bevy resources needed for Prediction

use crate::prediction_gate::predicts;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_connection::NetworkingState;

use crate::correction_policy::CorrectionPolicy;
use crate::rollback_config::RollbackConfig;
use campfire_net_core::NetTick;
use campfire_net_sync::InputTimelineConfig;

/// Application-global state that enables prediction and rollback.
///
/// `PredictionPlugin` installs the prediction systems, but
/// those systems only run when this resource is present and the cached network topology is a
/// client. Insert one manager into the application when it should
/// create predicted entities, record prediction history, and perform one global
/// rollback/reconciliation pipeline.
#[derive(Resource, Debug, Default, Reflect)]
pub struct PredictionManager {
    /// Configuration for how rollbacks are triggered
    pub rollback_config: RollbackConfig,
    /// Configuration for smoothing the rollback error over time
    pub correction_policy: CorrectionPolicy,
}

impl PredictionManager {
    /// How many ticks a rollback may go back: the policy's bound, within what the input
    /// timeline predicts.
    pub fn max_rollback_ticks(&self, input_config: &InputTimelineConfig) -> u16 {
        self.rollback_config
            .effective_max_rollback_ticks(input_config)
    }

    /// The oldest tick a rollback from `now` may restore.
    pub fn oldest_rollback_tick(
        &self,
        now: NetTick,
        input_config: &InputTimelineConfig,
    ) -> NetTick {
        now - u32::from(self.max_rollback_ticks(input_config))
    }

    /// Returns true if this application opted into prediction and has a supported active topology.
    pub(crate) fn should_run(
        manager: Option<Res<'_, Self>>,
        networking: Res<'_, NetworkingState>,
    ) -> bool {
        predicts(manager.is_some(), &networking)
    }
}

#[cfg(test)]
mod tests;
