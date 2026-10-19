use crate::link_estimator_plugin::LinkEstimatorPlugin;
use crate::sync_systems::SyncSystems;
use bevy_app::{App, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::schedule::IntoScheduleConfigs;
use campfire_net_timeline::TimelineSystems;
use campfire_net_transport::TransportSystems;

#[derive(Debug)]
pub(crate) struct TimelineSyncPlugin;

impl Plugin for TimelineSyncPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<LinkEstimatorPlugin>() {
            app.add_plugins(LinkEstimatorPlugin);
        }
        app.configure_sets(PostUpdate, SyncSystems::Sync);
        // A packet's sample measures against the timeline at its receipt, so it advances first.
        app.configure_sets(
            PreUpdate,
            TimelineSystems::Advance.before(TransportSystems::Receive),
        );
    }
}
