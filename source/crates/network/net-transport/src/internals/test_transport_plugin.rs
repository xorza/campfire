use crate::app_channel_ext::AppChannelExt;
use crate::channel_registry::ChannelRegistry;
use crate::transport_plugin::TransportPlugin;
use bevy_app::prelude::*;
use bevy_utils::default;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelConfig;
use campfire_net_channels::ChannelMode;

#[derive(Debug)]
pub struct TestChannel;
impl Channel for TestChannel {}

#[derive(Debug)]
pub struct TestTransportPlugin;

impl Plugin for TestTransportPlugin {
    fn build(&self, app: &mut App) {
        // add all channels before adding the TransportPlugin
        app.init_resource::<ChannelRegistry>();
        app.register_channel::<TestChannel>(ChannelConfig {
            mode: ChannelMode::UnorderedUnreliable,
            ..default()
        });
        // add the TransportPlugin
        app.add_plugins(TransportPlugin);
    }
}
