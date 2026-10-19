use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_net_core::TickRate;
use campfire_net_interpolation::internals::InterpolationStack;
use campfire_net_stepper::Stack;

use crate::fixtures::protocol::PredictionProtocolPlugin;
use crate::prediction_manager::PredictionManager;
use crate::prediction_marker_plugin::PredictionMarkerPlugin;
use crate::prediction_plugin::PredictionPlugin;
use crate::prediction_plugins::PredictionPlugins;
use crate::prediction_registry_plugin::PredictionRegistryPlugin;

/// Interpolation with prediction and rollback on the client.
#[derive(Debug, Clone, Copy, Default)]
pub struct PredictionStack;

impl PredictionStack {
    fn add_markers(app: &mut App) {
        if !app.is_plugin_added::<PredictionMarkerPlugin>() {
            app.add_plugins(PredictionMarkerPlugin);
        }
    }
}

impl Stack for PredictionStack {
    fn add_client(&self, app: &mut App, tick_rate: TickRate) {
        InterpolationStack.add_client(app, tick_rate);
        Self::add_markers(app);
        if !app.is_plugin_added::<PredictionPlugin>() {
            app.add_plugins((PredictionRegistryPlugin, PredictionPlugins));
        }
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        InterpolationStack.add_server(app, tick_rate);
        Self::add_markers(app);
    }

    fn add_protocol(&self, app: &mut App) {
        InterpolationStack.add_protocol(app);
        if !app.is_plugin_added::<PredictionProtocolPlugin>() {
            app.add_plugins(PredictionProtocolPlugin);
        }
    }

    fn client_link(&self, link: &mut EntityWorldMut<'_>) {
        InterpolationStack.client_link(link);
        link.world_scope(|world| world.insert_resource(PredictionManager::default()));
    }

    fn server_link(&self, link: &mut EntityWorldMut<'_>) {
        InterpolationStack.server_link(link);
    }

    fn is_ready(&self, world: &World) -> bool {
        InterpolationStack.is_ready(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_net_stepper::Stepper;
    use campfire_net_stepper::StepperConfig;

    /// The client connects and syncs with prediction on, its app holding the prediction manager.
    #[test]
    fn the_client_predicts_once_synced() {
        let stepper = Stepper::new(PredictionStack, StepperConfig::host_server());
        assert!(
            stepper
                .client_app(0)
                .world()
                .contains_resource::<PredictionManager>()
        );
    }
}
