use crate::channel_registry::ChannelRegistry;
use crate::transport_plugin::TransportPlugin;
use bevy_app::App;
use campfire_net_channels::Channel;
use campfire_net_connection::Client;
use campfire_net_connection::ClientOf;
use campfire_net_connection::LinkSide;
use campfire_net_core::LinkDirections;

#[derive(Debug)]
pub struct ChannelRegistration<'a, C> {
    pub app: &'a mut App,
    pub(crate) _marker: core::marker::PhantomData<C>,
}

impl<C: Channel> ChannelRegistration<'_, C> {
    /// Widens the directions the channel travels in.
    pub fn add_direction(&mut self, directions: impl Into<LinkDirections>) -> &mut Self {
        let directions = directions.into();
        self.app
            .world_mut()
            .resource_mut::<ChannelRegistry>()
            .add_direction::<C>(directions);
        self.add_side_direction::<Client>(directions);
        self.add_side_direction::<ClientOf>(directions);
        self
    }

    /// Gives each link of side `R` the channel's sender or receiver, as `directions` have it send
    /// or receive.
    fn add_side_direction<R: LinkSide>(&mut self, directions: LinkDirections) {
        if directions.contains(R::SENDS) {
            self.app
                .add_observer(TransportPlugin::add_sender_channel::<C, R>);
        }
        if directions.contains(R::RECEIVES) {
            self.app
                .add_observer(TransportPlugin::add_receiver_channel::<C, R>);
        }
    }
}
