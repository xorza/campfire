use crate::prediction_gate::predicts;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_connection::NetworkingState;

use crate::correction_policy::CorrectionPolicy;
use crate::rollback_setup::RollbackSetup;
use campfire_net_core::NetTick;
use campfire_net_sync::InputTimelineSetup;

/// The settings of prediction and rollback, whose presence turns them on: the prediction systems
/// run only while this resource exists and the app's network topology is a client.
#[derive(Resource, Debug, Default, Reflect)]
pub struct PredictionSetup {
    /// When a rollback starts.
    pub rollback_setup: RollbackSetup,
    /// How the error a rollback finds is smoothed over time.
    pub correction_policy: CorrectionPolicy,
}

impl PredictionSetup {
    /// How many ticks a rollback may go back: the policy's bound, within what the input
    /// timeline predicts.
    pub fn max_rollback_ticks(&self, input_setup: &InputTimelineSetup) -> u16 {
        self.rollback_setup
            .effective_max_rollback_ticks(input_setup)
    }

    /// The oldest tick a rollback from `now` may restore.
    pub fn oldest_rollback_tick(&self, now: NetTick, input_setup: &InputTimelineSetup) -> NetTick {
        now - u32::from(self.max_rollback_ticks(input_setup))
    }

    /// Returns true if this application opted into prediction and has a supported active topology.
    pub(crate) fn should_run(
        setup: Option<Res<'_, Self>>,
        networking: Res<'_, NetworkingState>,
    ) -> bool {
        predicts(setup.is_some(), &networking)
    }
}

#[cfg(test)]
mod tests;
