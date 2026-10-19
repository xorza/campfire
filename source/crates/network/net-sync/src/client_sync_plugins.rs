use bevy_app::{PluginGroup, PluginGroupBuilder};

use crate::client_sync_plugin::ClientSyncPlugin;
use crate::local_timeline_sync_plugin::LocalTimelineSyncPlugin;

/// A client's sync to its server, on [`SyncPlugins`](crate::sync_plugins::SyncPlugins): the
/// local timeline's discipline, then the estimate of the server's timeline.
#[derive(Debug)]
pub struct ClientSyncPlugins;

impl PluginGroup for ClientSyncPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(LocalTimelineSyncPlugin)
            .add(ClientSyncPlugin)
    }
}
