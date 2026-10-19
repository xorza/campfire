use crate::channel_registration::ChannelRegistration;
use crate::channel_registry::ChannelRegistry;
use bevy_app::App;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelConfig;

/// Registers the channels of the protocol.
pub trait AppChannelExt {
    /// Registers channel `C` with its config.
    fn register_channel<C: Channel>(&mut self, config: ChannelConfig)
    -> ChannelRegistration<'_, C>;
}

impl AppChannelExt for App {
    fn register_channel<C: Channel>(
        &mut self,
        config: ChannelConfig,
    ) -> ChannelRegistration<'_, C> {
        self.world_mut()
            .get_resource_or_init::<ChannelRegistry>()
            .register_channel::<C>(config);
        ChannelRegistration {
            app: self,
            _marker: core::marker::PhantomData,
        }
    }
}
