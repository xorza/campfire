use crate::endpoint_user::EndpointUser;
use crate::link_messages::{EndpointList, LinkMessages, ListedEndpoint};
use crate::message_type_id::MessageTypeId;
use crate::send_queue::Priority;
use crate::send_queue::{QueuedSender, SendQueue};
use bevy_ecs::prelude::*;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelTypeId;

/// The events of type `M` a link sends to its peer to trigger.
#[derive(Debug, Component)]
#[require(LinkMessages)]
#[component(on_add = LinkMessages::list::<EventSender<M>>, on_remove = LinkMessages::unlist::<EventSender<M>>)]
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
        self.queue.push(trigger, ChannelTypeId::of::<C>(), priority);
    }
}

impl<M: Event> QueuedSender<M> for EventSender<M> {
    fn queue_mut(&mut self) -> &mut SendQueue<M> {
        &mut self.queue
    }
}

impl<M: Event> ListedEndpoint for EventSender<M> {
    const LIST: EndpointList = EndpointList::Send(EndpointUser::Link);

    fn message_type_id() -> MessageTypeId {
        MessageTypeId::of::<M>()
    }
}

#[cfg(test)]
impl<M: Event> EventSender<M> {
    pub(crate) const fn len(&self) -> usize {
        self.queue.len()
    }
}
