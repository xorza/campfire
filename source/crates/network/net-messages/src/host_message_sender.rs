use crate::endpoint_side::EndpointSide;
use crate::message::Message;
use crate::message_kind::MessageKind;
use crate::message_manager::{Endpoint, MessageManager};
use crate::message_sender::MessageSender;
use crate::send_queue::{QueuedSender, SendQueue};
use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use core::ops::{Deref, DerefMut};

/// The sender of a host client's own side: what the host client sends its server.
///
/// A host client is also the server's link to it, whose [`MessageSender<M>`] sends toward the
/// host client, so the host client's systems send through this one. It has the API of
/// [`MessageSender<M>`].
#[derive(Debug, Component)]
#[require(MessageManager)]
#[component(on_add = HostMessageSender::<M>::on_add_hook, on_remove = HostMessageSender::<M>::on_remove_hook)]
pub struct HostMessageSender<M: Message>(MessageSender<M>);

impl<M: Message> Default for HostMessageSender<M> {
    fn default() -> Self {
        Self(MessageSender::default())
    }
}

impl<M: Message> Deref for HostMessageSender<M> {
    type Target = MessageSender<M>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<M: Message> DerefMut for HostMessageSender<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<M: Message> QueuedSender<M> for HostMessageSender<M> {
    fn queue_mut(&mut self) -> &mut SendQueue<M> {
        self.0.queue_mut()
    }
}

impl<M: Message> HostMessageSender<M> {
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
