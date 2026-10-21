use crate::hello_plugin::HelloPlugin;
use crate::pre_spawned_plugin::PreSpawnedPlugin;
use crate::replicon_channel_registration_plugin::RepliconChannelRegistrationPlugin;
use crate::shared_component_registration_plugin::SharedComponentRegistrationPlugin;
use bevy_app::PluginGroupBuilder;
use bevy_app::prelude::PluginGroup;
use bevy_replicon::shared::{AuthMethod, RepliconSharedPlugin};

#[derive(Debug)]
pub struct ReplicationPlugins;

impl PluginGroup for ReplicationPlugins {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        group = group.add(RepliconSharedPlugin {
            auth_method: AuthMethod::None,
        });
        group = group.add(RepliconChannelRegistrationPlugin);
        group = group.add(HelloPlugin);
        group = group.add(PreSpawnedPlugin);
        // Register shared marker components before server/client-specific plugins,
        // so that both sides have matching replicon component IDs.
        group = group.add(SharedComponentRegistrationPlugin);

        group
    }
}

#[cfg(test)]
mod tests;
