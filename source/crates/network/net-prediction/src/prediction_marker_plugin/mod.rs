use crate::prediction_plugin::{PRE_SPAWNED_PRIORITY, PREDICTED_PRIORITY, PREDICTED_SEND_PRIORITY};
use bevy_app::prelude::*;
use bevy_replicon::prelude::AppMarkerExt;
use bevy_replicon::shared::replication::receive_markers::MarkerConfig;
use campfire_net_replication::Predicted;
use campfire_net_replication::{PreSpawned, PredictedSend};

/// Registers Replicon's prediction markers for the shared client/server protocol.
///
/// Add this before registering predicted components. The stack's high-level
/// client and server plugin groups add it automatically.
#[derive(Debug, Default)]
pub struct PredictionMarkerPlugin;

impl Plugin for PredictionMarkerPlugin {
    fn build(&self, app: &mut App) {
        // A matched pre-spawn must preserve its locally predicted live value
        // even when PredictedSend is included in the same authoritative update.
        app.register_marker_with::<PreSpawned>(MarkerConfig {
            priority: PRE_SPAWNED_PRIORITY,
            need_history: true,
            affects_same_update: false,
        });
        // `PredictedSend` is replicated; it must be applied first so the rest
        // of the update uses the prediction receive functions.
        app.register_marker_with::<PredictedSend>(MarkerConfig {
            priority: PREDICTED_SEND_PRIORITY,
            need_history: true,
            affects_same_update: true,
        });
        // Keep the receiver-local marker registered for users that opt an
        // already replicated entity into prediction manually. It is never
        // replicated itself, so it stays out of the protocol hash.
        app.register_marker_with::<Predicted>(MarkerConfig {
            priority: PREDICTED_PRIORITY,
            need_history: true,
            affects_same_update: false,
        });
    }
}

#[cfg(test)]
mod tests;
