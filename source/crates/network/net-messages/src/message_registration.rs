use crate::message::Message;
use crate::message_registry::{MessageRegistry, mapped_cx_deserialize, mapped_cx_serialize};
use crate::message_sender::MessageSender;
use bevy_app::App;
use bevy_ecs::component::Component;
use bevy_ecs::entity::MapEntities;
use campfire_net_connection::Client;
use campfire_net_connection::ClientOf;
use campfire_net_connection::LinkSide;
use campfire_net_core::LinkDirections;

#[derive(Debug)]
pub struct MessageRegistration<'a, M> {
    pub app: &'a mut App,
    pub(crate) _marker: core::marker::PhantomData<M>,
}

impl<M: Message> MessageRegistration<'_, M> {
    /// Specify that the message contains entities which should be mapped from the remote world to
    /// the local world upon deserialization
    pub fn add_map_entities(&mut self) -> &mut Self
    where
        M: Clone + MapEntities + 'static,
    {
        let mut registry = self.app.world_mut().resource_mut::<MessageRegistry>();
        registry.add_map_entities::<M>(mapped_cx_serialize, mapped_cx_deserialize);
        self
    }

    /// Adds the sender component on each side that sends this message.
    ///
    /// Receiver components are inserted lazily when a payload arrives.
    pub fn add_direction(&mut self, directions: impl Into<LinkDirections>) -> &mut Self {
        add_senders::<M, MessageSender<M>>(self.app, directions.into());
        self
    }
}

/// Widens the directions `M` travels in, and gives each link of a side that sends it in
/// `directions` its sender `S`. A receiver goes on lazily when a payload arrives.
pub(crate) fn add_senders<M: 'static, S: Component + Default>(
    app: &mut App,
    directions: LinkDirections,
) {
    app.world_mut()
        .resource_mut::<MessageRegistry>()
        .add_direction::<M>(directions);
    if directions.contains(Client::SENDS) {
        app.try_register_required_components::<Client, S>().ok();
    }
    if directions.contains(ClientOf::SENDS) {
        app.try_register_required_components::<ClientOf, S>().ok();
    }
}
