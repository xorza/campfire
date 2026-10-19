use crate::endpoint_user::EndpointUser;
use crate::link_messages::{EndpointList, LinkMessages, ListedEndpoint};
use crate::message::Message;
use crate::message_sender::MessageSender;
use crate::message_type_id::MessageTypeId;
use crate::send_queue::{QueuedSender, SendQueue};
use bevy_ecs::component::Component;
use core::ops::{Deref, DerefMut};

/// The sender of a host client's own side: what the host client sends its server.
///
/// A host client is also the server's link to it, whose [`MessageSender<M>`] sends toward the
/// host client, so the host client's systems send through this one. It has the API of
/// [`MessageSender<M>`].
#[derive(Debug, Component)]
#[require(LinkMessages)]
#[component(on_add = LinkMessages::list::<HostMessageSender<M>>, on_remove = LinkMessages::unlist::<HostMessageSender<M>>)]
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

impl<M: Message> HostMessageSender<M> {}

impl<M: Message> ListedEndpoint for HostMessageSender<M> {
    const LIST: EndpointList = EndpointList::Send(EndpointUser::HostClient);

    fn message_type_id() -> MessageTypeId {
        MessageTypeId::of::<M>()
    }
}
