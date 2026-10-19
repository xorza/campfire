use crate::endpoint_side::EndpointSide;
use crate::message_kind::MessageKind;
use crate::message_manager::{Endpoint, MessageManager};
use crate::send_queue::Priority;
use crate::send_queue::{QueuedSender, SendQueue};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelKind;

/// Component used to send triggers of type `M` remotely.
#[derive(Debug, Component)]
#[require(MessageManager)]
#[component(on_add = EventSender::<M>::on_add_hook, on_remove = EventSender::<M>::on_remove_hook)]
pub struct EventSender<M: Event> {
    queue: SendQueue<M>,
}

impl<M: Event> Default for EventSender<M> {
    fn default() -> Self {
        Self {
            queue: SendQueue::default(),
        }
    }
}

impl<M: Event> EventSender<M> {
    /// Buffers a trigger `M` to be sent over the specified channel to the target entities.
    pub fn trigger<C: Channel>(&mut self, trigger: M) {
        self.trigger_with_priority::<C>(trigger, 1.0);
    }

    /// Buffers an event with an explicit bandwidth priority.
    pub fn trigger_with_priority<C: Channel>(&mut self, trigger: M, priority: Priority) {
        self.queue.push(trigger, ChannelKind::of::<C>(), priority);
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

impl<M: Event> QueuedSender<M> for EventSender<M> {
    fn queue_mut(&mut self) -> &mut SendQueue<M> {
        &mut self.queue
    }
}

#[cfg(test)]
impl<M: Event> EventSender<M> {
    pub(crate) const fn len(&self) -> usize {
        self.queue.len()
    }
}
