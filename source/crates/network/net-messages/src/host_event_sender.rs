use crate::endpoint_side::EndpointSide;
use crate::event_sender::EventSender;
use crate::message_kind::MessageKind;
use crate::message_manager::{Endpoint, MessageManager};
use crate::send_queue::{QueuedSender, SendQueue};
use bevy_ecs::component::Component;
use bevy_ecs::event::Event;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use core::ops::{Deref, DerefMut};

/// The event sender of a host client's own side: what the host client triggers on its server.
///
/// A host client is also the server's link to it, whose [`EventSender<M>`] triggers toward the
/// host client, so the host client's systems trigger through this one. It has the API of
/// [`EventSender<M>`].
#[derive(Debug, Component)]
#[require(MessageManager)]
#[component(on_add = HostEventSender::<M>::on_add_hook, on_remove = HostEventSender::<M>::on_remove_hook)]
pub struct HostEventSender<M: Event>(EventSender<M>);

impl<M: Event> Default for HostEventSender<M> {
    fn default() -> Self {
        Self(EventSender::default())
    }
}

impl<M: Event> Deref for HostEventSender<M> {
    type Target = EventSender<M>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<M: Event> DerefMut for HostEventSender<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<M: Event> QueuedSender<M> for HostEventSender<M> {
    fn queue_mut(&mut self) -> &mut SendQueue<M> {
        self.0.queue_mut()
    }
}

impl<M: Event> HostEventSender<M> {
    fn on_add_hook(world: DeferredWorld<'_>, context: HookContext) {
        MessageManager::on_endpoint_add(
            world,
            context,
            Endpoint::Send(EndpointSide::HostClient),
            MessageKind::of::<M>(),
        );
    }

    fn on_remove_hook(world: DeferredWorld<'_>, context: HookContext) {
        MessageManager::on_endpoint_remove(
            world,
            context,
            Endpoint::Send(EndpointSide::HostClient),
            MessageKind::of::<M>(),
        );
    }
}
