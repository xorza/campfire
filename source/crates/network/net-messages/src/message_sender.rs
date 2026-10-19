use crate::endpoint_side::EndpointSide;
use crate::message::Message;
use crate::message_kind::MessageKind;
use crate::message_manager::Endpoint;
use crate::message_manager::MessageManager;
use crate::send_queue::Priority;
use crate::send_queue::QueuedSender;
use crate::send_queue::SendQueue;
use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelKind;

/// A component that allows an entity to send messages of type `M` over the network.
///
/// You can send a message by simply buffering the messages into the `MessageSender<M>` component
/// using the `send` or `send_with_priority` methods.
///
/// ```rust
/// # use bevy_ecs::prelude::*;
/// # use campfire_net_messages::{AppMessageExt, AppEventExt, MessageSender};
/// # use campfire_net_channels::Channel;
///
/// struct M;
///
/// struct MyChannel;
/// impl Channel for MyChannel {}
///
/// let mut message_sender = MessageSender::<M>::default();
/// message_sender.send::<MyChannel>(M);
/// ```
#[derive(Debug, Component)]
#[component(on_add = MessageSender::<M>::on_add_hook, on_remove = MessageSender::<M>::on_remove_hook)]
#[require(MessageManager)]
pub struct MessageSender<M: Message> {
    queue: SendQueue<M>,
}

impl<M: Message> Default for MessageSender<M> {
    fn default() -> Self {
        Self {
            queue: SendQueue::default(),
        }
    }
}

impl<M: Message> MessageSender<M> {
    /// Buffers a message to be sent over the channel
    pub fn send_with_priority<C: Channel>(&mut self, message: M, priority: Priority) {
        self.queue.push(message, ChannelKind::of::<C>(), priority);
    }

    /// Buffers a message to be sent over the channel
    pub fn send<C: Channel>(&mut self, message: M) {
        self.send_with_priority::<C>(message, 1.0);
    }

    fn on_add_hook(world: DeferredWorld<'_>, context: HookContext) {
        MessageManager::on_endpoint_add(
            world,
            context,
            Endpoint::Send(EndpointSide::Link),
            MessageKind::of::<M>(),
        );
    }

    fn on_remove_hook(world: DeferredWorld<'_>, context: HookContext) {
        MessageManager::on_endpoint_remove(
            world,
            context,
            Endpoint::Send(EndpointSide::Link),
            MessageKind::of::<M>(),
        );
    }
}

impl<M: Message> QueuedSender<M> for MessageSender<M> {
    fn queue_mut(&mut self) -> &mut SendQueue<M> {
        &mut self.queue
    }
}

#[cfg(test)]
impl<M: Message> MessageSender<M> {
    pub(crate) const fn len(&self) -> usize {
        self.queue.len()
    }
}
