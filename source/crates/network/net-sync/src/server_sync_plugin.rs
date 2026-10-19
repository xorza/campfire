use bevy_app::{App, Plugin};

use crate::timeline_sync_plugin::TimelineSyncPlugin;

#[derive(Debug)]
pub struct ServerSyncPlugin;

impl Plugin for ServerSyncPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TimelineSyncPlugin>() {
            app.add_plugins(TimelineSyncPlugin);
        }
    }
}
