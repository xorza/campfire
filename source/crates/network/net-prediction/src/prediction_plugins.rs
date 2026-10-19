use crate::prediction_diagnostics_plugin::PredictionDiagnosticsPlugin;
use crate::prediction_plugin::PredictionPlugin;
use crate::rollback_plugin::RollbackPlugin;
use crate::timeline_switch_plugin::TimelineSwitchPlugin;
use bevy_app::{PluginGroup, PluginGroupBuilder};
use campfire_net_interpolation::FrameInterpolationPlugin;

/// A client's prediction and rollback, on the shared registries of prediction and interpolation:
/// the frame interpolation a correction measures each error against, the prediction systems, the
/// timeline switch, their diagnostics and the rollback.
#[derive(Debug)]
pub struct PredictionPlugins;

impl PluginGroup for PredictionPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(FrameInterpolationPlugin)
            .add(PredictionPlugin)
            .add(TimelineSwitchPlugin)
            .add(PredictionDiagnosticsPlugin::default())
            .add(RollbackPlugin)
    }
}
