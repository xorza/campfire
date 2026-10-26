use bevy_app::{PluginGroup, PluginGroupBuilder};

use crate::timeline_sync_plugin::TimelineSyncPlugin;

/// What both sides of a link sync: the sets of the timelines' sync.
#[derive(Debug)]
pub struct SyncPlugins;

impl PluginGroup for SyncPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>().add(TimelineSyncPlugin)
    }
}
