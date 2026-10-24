use crate::link_messages::LinkMessages;
use crate::message::Message;
use crate::message_registry::MessageRegistry;
use crate::message_type_id::MessageTypeId;
use crate::replicon_map_param::RepliconMapParam;
use crate::send_map_view::SendMapView;
use crate::send_queue::Priority;
use bevy_ecs::query::{Has, QueryFilter, With};
use bevy_ecs::{
    entity::EntitySet,
    system::{Local, Query, Res, SystemParam},
};
use bytes::Bytes;
use campfire_net_bytes::Writer;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelTypeId;
use campfire_net_connection::Client;
use campfire_net_connection::Connected;
use campfire_net_core::LinkDirection;
use campfire_net_transport::ChannelRegistry;
use campfire_net_transport::Transport;

/// SystemParam that sends one message or event to several links at once, serializing it once
/// when it maps no entities.
///
/// It addresses a set of links. A registered event arrives as a [`RemoteEvent`](crate::remote_event::RemoteEvent), as an
/// [`EventSender`](crate::event_sender::EventSender)'s does.
///
/// Each link sends from its own side, as its
/// [`MessageSender`](crate::message_sender::MessageSender) does. A message sent against its
/// registered direction is a bug: a debug build panics, and a release build drops it, as a peer
/// would unlink a sender that broke the protocol.
///
/// Only connected links receive. Entities map as a link's own sends map them: on a client
/// only, through replicon's map, then the link's own pairs.
#[derive(SystemParam)]
pub struct MultiMessageSender<'w, 's, F: QueryFilter + 'static = ()> {
    query: Query<
        'w,
        's,
        (&'static LinkMessages, &'static mut Transport, Has<Client>),
        (F, With<Connected>),
    >,
    replicon: RepliconMapParam<'w, 's>,
    registry: Res<'w, MessageRegistry>,
    channels: Res<'w, ChannelRegistry>,
    writer: Local<'s, Writer>,
}

/// A query filter is not `Debug`, so the filter takes no part in it.
impl<F: QueryFilter> core::fmt::Debug for MultiMessageSender<'_, '_, F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MultiMessageSender")
            .field("registry", &self.registry)
            .finish_non_exhaustive()
    }
}

impl<F: QueryFilter> MultiMessageSender<'_, '_, F> {
    /// Sends `message` to each of `links` that is connected.
    pub fn send<M: Message, C: Channel>(&mut self, message: &M, links: impl EntitySet) {
        self.send_with_priority::<M, C>(message, links, 1.0);
    }

    /// Sends `message` to each of `links` that is connected, at a bandwidth `priority`.
    pub fn send_with_priority<M: Message, C: Channel>(
        &mut self,
        message: &M,
        links: impl EntitySet,
        priority: Priority,
    ) {
        let channel = self
            .channels
            .net_id(ChannelTypeId::of::<C>())
            .expect("a message is sent on a registered channel");
        let registered = self.registry.message(
            self.registry
                .net_id(MessageTypeId::of::<M>())
                .expect("a message is registered before it is sent"),
        );
        let maps_entities = registered.serialize_fns.maps_entities;
        let travels = |direction| registered.travels(direction);
        // The bytes as this world names its entities, serialized once for every link of a server
        // that takes them so.
        let mut as_is: Option<Bytes> = None;
        for (link_messages, mut transport, is_client) in
            self.query.iter_many_unique_mut(links).matched()
        {
            let sends = if is_client {
                LinkDirection::ClientToServer
            } else {
                LinkDirection::ServerToClient
            };
            debug_assert!(
                travels(sends),
                "{} is sent {sends:?}, against its registered direction",
                core::any::type_name::<M>()
            );
            if !travels(sends) {
                continue;
            }
            let bytes = if maps_entities && is_client {
                let view = self
                    .replicon
                    .send_view(link_messages.entity_mapper.local_to_remote());
                self.registry
                    .serialize::<M>(message, &mut self.writer, &view);
                self.writer.take_written()
            } else if let Some(bytes) = &as_is {
                bytes.clone()
            } else {
                self.registry
                    .serialize::<M>(message, &mut self.writer, &SendMapView::AsIs);
                as_is.insert(self.writer.take_written()).clone()
            };
            transport.send(channel, bytes, priority);
        }
    }
}

#[cfg(test)]
mod tests;
