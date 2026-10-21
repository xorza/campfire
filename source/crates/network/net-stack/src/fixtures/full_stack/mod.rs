use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimePlugin;
use campfire_app_threads::OneThreadPlugins;
use campfire_math::TickRate;
use campfire_net_prediction::internals::PredictionStack;
use campfire_net_replication::RoomPlugin;
use campfire_net_stepper::Stack;

use crate::client_plugins::ClientPlugins;
use crate::server_plugins::ServerPlugins;

/// Every layer through the stack's plugin groups, with every layer's test protocol
/// (11-net-stack/testing.md, The full stack's home is `net-stack`).
#[derive(Debug, Clone, Copy, Default)]
pub struct FullStack;

impl FullStack {
    /// Bevy's transforms, without the marks of changed trees: with `multi_threaded`, which the
    /// workspace's Bevy has, the mark spawns its tasks in every frame, Bevy's allocations and not
    /// the stack's, which the allocation audits would count; a full propagation gives the same
    /// transforms.
    fn add_bevy(app: &mut App) {
        app.add_plugins((OneThreadPlugins, TimePlugin, TransformPlugin, StatesPlugin))
            .insert_resource(StaticTransformOptimizations::Disabled);
    }
}

impl Stack for FullStack {
    fn add_client(&self, app: &mut App, _: usize, tick_rate: TickRate) {
        Self::add_bevy(app);
        app.add_plugins(ClientPlugins { tick_rate });
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        Self::add_bevy(app);
        app.add_plugins((ServerPlugins { tick_rate }, RoomPlugin));
    }

    fn add_protocol(&self, app: &mut App) {
        PredictionStack.add_protocol(app);
    }

    fn client_link(&self, link: &mut EntityWorldMut<'_>) {
        PredictionStack.client_link(link);
    }

    fn server_link(&self, link: &mut EntityWorldMut<'_>) {
        PredictionStack.server_link(link);
    }

    fn is_ready(&self, world: &World) -> bool {
        PredictionStack.is_ready(world)
    }
}

#[cfg(test)]
mod tests;
