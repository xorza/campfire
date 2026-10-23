use crate::link_messages::LinkMessages;
use crate::link_messages::{EndpointList, ListedEndpoint};
use crate::message::Message;
use crate::message_error::MessageError;
use crate::message_receive_cx::MessageReceiveCx;
use crate::message_type_id::MessageTypeId;
use crate::messages_unread::MessagesUnread;
use crate::received_message::ReceivedMessage;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::Commands;
use bevy_ecs::world::FilteredEntityMut;
use bevy_ecs::world::World;
use bevy_utils::prelude::DebugName;
use campfire_log::LogEvent;
use tracing::trace;

/// The messages of type `M` a link received this frame, which [`receive`](Self::receive) drains
/// and `Last` clears.
#[derive(Debug, Component)]
#[require(LinkMessages)]
#[component(on_add = LinkMessages::list::<MessageReceiver<M>>, on_remove = LinkMessages::unlist::<MessageReceiver<M>>)]
pub struct MessageReceiver<M: Message> {
    ready: Vec<ReceivedMessage<M>>,
}

impl<M: Message> Default for MessageReceiver<M> {
    fn default() -> Self {
        Self { ready: Vec::new() }
    }
}

impl<M: Message> MessageReceiver<M> {
    const fn ready(&self) -> &Vec<ReceivedMessage<M>> {
        &self.ready
    }

    const fn ready_mut(&mut self) -> &mut Vec<ReceivedMessage<M>> {
        &mut self.ready
    }

    pub const fn has_messages(&self) -> bool {
        !self.ready().is_empty()
    }

    /// Take all messages from the [`MessageReceiver<M>`], deserialize them, and return them
    pub fn receive(&mut self) -> impl Iterator<Item = M> {
        self.ready_mut().drain(..).map(|m| m.data)
    }

    /// Take all messages from the [`MessageReceiver<M>`], deserialize them, and return them
    pub fn receive_with_tick(&mut self) -> impl Iterator<Item = ReceivedMessage<M>> {
        self.ready_mut().drain(..)
    }

    pub const fn len(&self) -> usize {
        self.ready().len()
    }

    pub const fn is_empty(&self) -> bool {
        self.ready().is_empty()
    }

    pub(crate) fn push_received(&mut self, message: ReceivedMessage<M>) {
        self.ready.push(message);
    }
}

impl<M: Message> MessageReceiver<M> {
    /// Pushes `message` into the receiver of `entity`, which goes on the entity first if it is not
    /// there yet.
    fn queue_received(
        commands: &mut Commands<'_, '_>,
        entity: Entity,
        message: ReceivedMessage<M>,
    ) {
        commands.queue(move |world: &mut World| {
            if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                entity_mut
                    .entry::<Self>()
                    .or_default()
                    .get_mut()
                    .push_received(message);
            }
        });
    }

    /// Receive a single message of type `M` from the channel, into its receiver among
    /// `receivers`, or into one that goes on with the commands when absent.
    pub(crate) fn receive_message_typed(
        receivers: &mut FilteredEntityMut<'_, '_>,
        cx: MessageReceiveCx<'_, '_, '_>,
    ) -> Result<(), MessageError> {
        let MessageReceiveCx {
            commands,
            link: entity,
            reader,
            header,
            channel_name,
            serialize_fns,
            entity_map,
            ..
        } = cx;
        let receiver = receivers.get_mut::<Self>();
        let insert_receiver = receiver.is_none();
        let data = serialize_fns.deserialize::<M>(reader, entity_map)?;
        let message = ReceivedMessage { data, header };
        if let Some(mut receiver) = receiver {
            receiver.push_received(message);
        } else {
            Self::queue_received(commands, entity, message);
        }
        trace!(
            "Received message {:?} on channel {:?}",
            DebugName::type_name::<M>(),
            header.channel_type_id
        );
        trace!(
            target: "campfire_net_debug::message",
            kind = "message_receive_typed",
            message_name = core::any::type_name::<M>(),
            channel = channel_name,
            remote_tick = header.remote_tick.0,
            message_id = ?header.message_id,
            insert_receiver,
            "deserialized message into receiver"
        );
        Ok(())
    }

    /// Clears the receiver among `receivers`, which the link lists.
    pub(crate) fn clear_typed(receivers: &mut FilteredEntityMut<'_, '_>) {
        let mut receiver = receivers
            .get_mut::<Self>()
            .expect("a link holds each receiver it lists");
        if receiver.has_messages() {
            let message_type = DebugName::type_name::<M>().to_string();
            MessagesUnread { message_type }.log();
            receiver.ready_mut().clear();
        }
    }
}

impl<M: Message> ListedEndpoint for MessageReceiver<M> {
    const LIST: EndpointList = EndpointList::Receive;

    fn message_type_id() -> MessageTypeId {
        MessageTypeId::of::<M>()
    }
}

#[cfg(test)]
mod tests;
