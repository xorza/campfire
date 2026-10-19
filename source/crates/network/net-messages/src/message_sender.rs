use crate::endpoint_user::EndpointUser;
use crate::link_messages::LinkMessages;
use crate::link_messages::{EndpointList, ListedEndpoint};
use crate::message::Message;
use crate::message_type_id::MessageTypeId;
use crate::send_queue::Priority;
use crate::send_queue::QueuedSender;
use crate::send_queue::SendQueue;
use bevy_ecs::component::Component;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelTypeId;

/// The messages of type `M` a link sends, buffered by `send` until the flush.
#[derive(Debug, Component)]
#[component(on_add = LinkMessages::list::<MessageSender<M>>, on_remove = LinkMessages::unlist::<MessageSender<M>>)]
#[require(LinkMessages)]
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
        self.queue.push(message, ChannelTypeId::of::<C>(), priority);
    }

    /// Buffers a message to be sent over the channel
    pub fn send<C: Channel>(&mut self, message: M) {
        self.send_with_priority::<C>(message, 1.0);
    }
}

impl<M: Message> QueuedSender<M> for MessageSender<M> {
    fn queue_mut(&mut self) -> &mut SendQueue<M> {
        &mut self.queue
    }
}

impl<M: Message> ListedEndpoint for MessageSender<M> {
    const LIST: EndpointList = EndpointList::Send(EndpointUser::Link);

    fn message_type_id() -> MessageTypeId {
        MessageTypeId::of::<M>()
    }
}

#[cfg(test)]
impl<M: Message> MessageSender<M> {
    pub(crate) const fn len(&self) -> usize {
        self.queue.len()
    }
}
