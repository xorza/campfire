use crate::channel_registry::{ChannelGate, ChannelRegistry};
use crate::transport_plugin::TransportPlugin;
use bevy_app::App;
use bevy_ecs::component::Component;
use campfire_net_channels::Channel;
use campfire_net_connection::Client;
use campfire_net_connection::ClientOf;
use campfire_net_core::LinkDirections;
use core::any::TypeId;

#[derive(Debug)]
pub struct ChannelRegistration<'a, C> {
    pub app: &'a mut App,
    pub(crate) _marker: core::marker::PhantomData<C>,
}

impl<C: Channel> ChannelRegistration<'_, C> {
    /// Makes the layer that registers the channel its one reader, which the message layer then
    /// leaves (11-net-stack/transport.md, A channel has one reader).
    pub fn read_by_owner(&mut self) -> &mut Self {
        self.app
            .world_mut()
            .resource_mut::<ChannelRegistry>()
            .read_by_owner::<C>();
        self
    }

    /// Widens the directions the channel travels in. Each link gets the channel's sender as its
    /// side sends on it, and its receiver as its side receives on it, as its transport comes.
    pub fn add_direction(&mut self, directions: impl Into<LinkDirections>) -> &mut Self {
        self.app
            .world_mut()
            .resource_mut::<ChannelRegistry>()
            .add_direction::<C>(directions.into(), ChannelGate::Open);
        self
    }

    /// Widens the directions the channel travels in, as [`Self::add_direction`] does, but gives
    /// a link of a side that receives on it its receiver only once the link holds `G`, the app's
    /// mark that it admitted the link: before, a packet on the channel unlinks the peer as a
    /// protocol violation, and nothing on it waits in the link's receive budget
    /// (11-net-stack/transport.md, A channel opens to a link the app admits). The receiver stays
    /// for the link's life, as `G` goes and through a reset of its transport.
    ///
    /// A channel opens behind one gate: its registration with another, or with none, panics.
    pub fn add_direction_after<G: Component>(
        &mut self,
        directions: impl Into<LinkDirections>,
    ) -> &mut Self {
        let gate = ChannelGate::After(TypeId::of::<G>());
        let mut registry = self.app.world_mut().resource_mut::<ChannelRegistry>();
        let first = !registry.gates(gate);
        registry.add_direction::<C>(directions.into(), gate);
        if first {
            self.app
                .add_observer(TransportPlugin::on_gate::<Client, G>)
                .add_observer(TransportPlugin::on_gate::<ClientOf, G>)
                .add_observer(TransportPlugin::on_gated_transport::<Client, G>)
                .add_observer(TransportPlugin::on_gated_transport::<ClientOf, G>);
        }
        self
    }
}
