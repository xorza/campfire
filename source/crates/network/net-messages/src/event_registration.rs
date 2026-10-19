use crate::event_sender::EventSender;
use crate::host_event_sender::HostEventSender;
use crate::message_registration;
use crate::message_registry::{
    MessageRegistry, mapped_context_deserialize, mapped_context_serialize,
};
use bevy_app::App;
use bevy_ecs::{entity::MapEntities, event::Event};
use campfire_net_core::Directions;

#[derive(Debug)]
pub struct EventRegistration<'a, M> {
    pub app: &'a mut App,
    pub(crate) _marker: core::marker::PhantomData<M>,
}

impl<M: Event> EventRegistration<'_, M> {
    /// Specify that the Event contains entities which should be mapped from the remote world to the
    /// local world upon deserialization
    pub fn add_map_entities(&mut self) -> &mut Self
    where
        M: Clone + MapEntities + 'static,
    {
        let mut registry = self.app.world_mut().resource_mut::<MessageRegistry>();
        registry.add_map_entities::<M>(mapped_context_serialize, mapped_context_deserialize);
        self
    }

    /// Adds the event sender component on each side that sends this event.
    pub fn add_direction(&mut self, directions: impl Into<Directions>) -> &mut Self {
        message_registration::add_senders::<M, EventSender<M>, HostEventSender<M>>(
            self.app,
            directions.into(),
        );
        self
    }
}
