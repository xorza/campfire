use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use campfire_net_core::TickRate;
use campfire_net_prediction::internals::PredictionStack;
use campfire_net_replication::RoomPlugin;
use campfire_net_stepper::Stack;

use bevy::app::PluginGroupBuilder;
use campfire_net_link::ServerLinkPlugin;

use crate::client_plugins::ClientPlugins;
use crate::layers;
use crate::server_plugins::ServerPlugins;

/// Every layer through the stack's plugin groups, with every layer's test protocol
/// (13-net-stack/testing.md, The full stack's home is `net-stack`).
#[derive(Debug, Clone, Copy, Default)]
pub struct FullStack;

impl FullStack {
    fn add_bevy(app: &mut App) {
        if !app.is_plugin_added::<TransformPlugin>() {
            app.add_plugins((TransformPlugin, StatesPlugin));
        }
    }
}

impl Stack for FullStack {
    /// A server app takes the client's own layers alone for its host client, as `HostPlugins`
    /// lists them, since the stepper adds its server first.
    fn add_client(&self, app: &mut App, tick_rate: TickRate) {
        Self::add_bevy(app);
        if app.is_plugin_added::<ServerLinkPlugin>() {
            app.add_plugins(layers::client(PluginGroupBuilder::start::<ClientPlugins>()));
        } else {
            app.add_plugins(ClientPlugins { tick_rate });
        }
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
mod tests {
    use super::*;
    use campfire_net_connection::Connected;
    use campfire_net_stepper::AppId;
    use campfire_net_stepper::Stepper;
    use campfire_net_stepper::StepperSetup;
    use campfire_net_sync::internals::input_timeline_is_synced;
    use core::time::Duration;

    fn is_synced_and_connected(stepper: &Stepper<FullStack>) -> bool {
        input_timeline_is_synced(stepper.client_app(0).world())
            && stepper.client(0).contains::<Connected>()
    }

    #[test]
    fn a_client_on_a_faster_frame_rate_stays_synced() {
        let mut stepper = Stepper::new(FullStack, StepperSetup::single());
        // About 144 Hz on the client against the server's 100 Hz ticks.
        stepper.set_frame_interval(AppId::Client(0), Duration::from_millis(7));
        let start_tick = stepper.server_tick();
        stepper.run_for(Duration::from_secs(2));
        assert!(is_synced_and_connected(&stepper));
        // 2 s at the 10 ms tick is 200 server ticks.
        assert_eq!(stepper.server_tick() - start_tick, 200);
    }

    #[test]
    fn a_client_whose_clock_drifts_stays_synced() {
        let mut stepper = Stepper::new(FullStack, StepperSetup::single());
        // 0.1 % fast: 4 ms over the 4 s run.
        stepper.set_clock_drift(AppId::Client(0), 1000);
        stepper.run_for(Duration::from_secs(4));
        assert!(is_synced_and_connected(&stepper));
    }

    #[test]
    fn a_stalled_client_syncs_again() {
        let mut stepper = Stepper::new(FullStack, StepperSetup::single());
        let frames_before = stepper.client_tick(0);
        // A 2 s stall: Bevy advances the client's virtual time by at most 250 ms on the frame it
        // resumes, so its ticks fall behind the server's and the sync must bring them back.
        stepper.stall(AppId::Client(0), Duration::from_secs(2));
        stepper.run_for(Duration::from_millis(1990));
        assert_eq!(
            stepper.client_tick(0),
            frames_before,
            "a stalled client runs no frame"
        );
        stepper.run_for(Duration::from_secs(3));
        assert!(is_synced_and_connected(&stepper));
        // The synced client leads the server, which ran on through the stall.
        assert!(stepper.client_tick(0) >= stepper.server_tick());
    }
}
