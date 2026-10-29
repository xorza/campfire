use bevy_app::{App, Plugin};

use crate::prediction_registry::{self, PredictionRegistry};

/// Holds the prediction registry, which a shared protocol registers into on both sides, and seals
/// it once the plugins are built.
#[derive(Debug, Default)]
pub struct PredictionRegistryPlugin;

impl Plugin for PredictionRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PredictionRegistry>();
    }

    fn finish(&self, app: &mut App) {
        prediction_registry::seal(app);
    }
}
