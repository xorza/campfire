use crate::channel_registry::ChannelRegistry;
use crate::transport_plugin::TransportPlugin;
use bevy_app::App;
use bevy_ecs::component::Component;
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

    /// Widens the directions the channel travels in, as [`Self::add_direction`] does, but gives
    /// a link of a side that receives on it its receiver only once the link holds `G`, the app's
    /// mark that it admitted the link: before, a packet on the channel unlinks the peer as a
    /// protocol violation, and nothing on it waits in the link's receive budget
    /// (11-net-stack/transport.md, A channel opens to a link the app admits). The receiver stays
    /// for the link's life, as `G` goes and through a reset of its transport.
    pub fn add_direction_after<G: Component>(
        &mut self,
        directions: impl Into<LinkDirections>,
    ) -> &mut Self {
        let directions = directions.into();
        self.app
            .world_mut()
            .resource_mut::<ChannelRegistry>()
            .add_direction::<C>(directions);
        self.add_side_direction_after::<Client, G>(directions);
        self.add_side_direction_after::<ClientOf, G>(directions);
        self
    }

    /// Gives each link of side `R` the channel's sender as `directions` have it send, and its
    /// receiver once the link holds `G` as they have it receive.
    fn add_side_direction_after<R: LinkSide, G: Component>(&mut self, directions: LinkDirections) {
        if directions.contains(R::SENDS) {
            self.app
                .add_observer(TransportPlugin::add_sender_channel::<C, R>);
        }
        if directions.contains(R::RECEIVES) {
            self.app
                .add_observer(TransportPlugin::open_receiver_channel::<C, R, G>)
                .add_observer(TransportPlugin::open_admitted_receiver_channel::<C, R, G>);
        }
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
