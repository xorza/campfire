use crate::endpoint_user::EndpointUser;
use crate::event_sender::EventSender;
use crate::link_messages::{EndpointList, LinkMessages, ListedEndpoint};
use crate::message_type_id::MessageTypeId;
use crate::send_queue::{QueuedSender, SendQueue};
use bevy_ecs::component::Component;
use bevy_ecs::event::Event;
use core::ops::{Deref, DerefMut};

/// The event sender of a host client's own side: what the host client triggers on its server.
///
/// A host client is also the server's link to it, whose [`EventSender<M>`] triggers toward the
/// host client, so the host client's systems trigger through this one. It has the API of
/// [`EventSender<M>`].
#[derive(Debug, Component)]
#[require(LinkMessages)]
#[component(on_add = LinkMessages::list::<HostEventSender<M>>, on_remove = LinkMessages::unlist::<HostEventSender<M>>)]
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

impl<M: Event> HostEventSender<M> {}

impl<M: Event> ListedEndpoint for HostEventSender<M> {
    const LIST: EndpointList = EndpointList::Send(EndpointUser::HostClient);

    fn message_type_id() -> MessageTypeId {
        MessageTypeId::of::<M>()
    }
}
