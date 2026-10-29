use bevy_app::{App, Plugin};
use bevy_replicon::prelude::AppMarkerExt;
use bevy_replicon::shared::replication::receive_markers::MarkerConfig;
use campfire_net_replication::Interpolated;
use campfire_net_replication::InterpolatedSend;
use campfire_net_replication::MarkerPriority;

/// Registers Replicon's interpolation markers for the shared client/server protocol.
///
/// Add this before registering interpolated components. The stack's high-level
/// client and server plugin groups add it automatically.
#[derive(Debug, Default)]
pub struct InterpolationMarkerPlugin;

impl Plugin for InterpolationMarkerPlugin {
    fn build(&self, app: &mut App) {
        // `InterpolatedSend` is replicated; it must be applied first so the rest
        // of the update uses the interpolation receive functions.
        app.register_marker_with::<InterpolatedSend>(MarkerConfig {
            priority: MarkerPriority::INTERPOLATED,
            need_history: true,
            affects_same_update: true,
        });
        app.register_marker_with::<Interpolated>(MarkerConfig {
            priority: MarkerPriority::INTERPOLATED,
            need_history: true,
            affects_same_update: false,
        });
    }
}

#[cfg(test)]
mod tests;
