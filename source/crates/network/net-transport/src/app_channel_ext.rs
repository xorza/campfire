use bevy_app::App;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelSetup;

use crate::channel_registration::ChannelRegistration;
use crate::channel_registry::ChannelRegistry;

/// Registers the channels of the protocol.
pub trait AppChannelExt {
    /// Registers channel `C` with its setup.
    fn register_channel<C: Channel>(&mut self, setup: ChannelSetup) -> ChannelRegistration<'_, C>;
}

impl AppChannelExt for App {
    fn register_channel<C: Channel>(&mut self, setup: ChannelSetup) -> ChannelRegistration<'_, C> {
        self.world_mut()
            .get_resource_or_init::<ChannelRegistry>()
            .register_channel::<C>(setup);
        ChannelRegistration {
            app: self,
            _marker: core::marker::PhantomData,
        }
    }
}
