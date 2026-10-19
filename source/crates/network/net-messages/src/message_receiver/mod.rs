use crate::endpoint_user::EndpointUser;
use crate::link_messages::LinkMessages;
use crate::link_messages::{EndpointList, ListedEndpoint};
use crate::message::Message;
use crate::message_error::MessageError;
use crate::message_receive_cx::MessageReceiveCx;
use crate::message_receive_cx::ReceivedMessageHeader;
use crate::message_type_id::MessageTypeId;
use crate::received_message::ReceivedMessage;
use bevy_ecs::change_detection::Mut;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::ParallelCommands;
use bevy_ecs::world::FilteredEntityMut;
use bevy_ecs::world::World;
use bevy_utils::prelude::DebugName;
use core::ops::Deref;
use core::ops::DerefMut;
use tracing::trace;
use tracing::warn;

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

    /// Mutate and/or drop the buffered messages in place, *before* they are
    /// consumed by the receiving system.
    ///
    /// This is the hook for validation/sanitization systems that run between
    /// message receipt and whatever consumes the messages (e.g. server-side
    /// input validation between `MessageSystems::Receive` and the input-buffer
    /// apply). Returning `false` from `keep` drops that message; mutating the
    /// `&mut M` rewrites it. The message header (remote tick, channel,
    /// message id) is preserved automatically — unlike drain-then-re-push.
    pub fn retain_messages(&mut self, mut keep: impl FnMut(&mut M) -> bool) {
        self.ready_mut()
            .retain_mut(|received| keep(&mut received.data));
    }

    /// Like [`retain_messages`](Self::retain_messages), but the predicate also
    /// gets the per-message [`ReceivedMessageHeader`] (`remote_tick`, channel, and
    /// message id) that `retain_messages` hides.
    ///
    /// Use this when validation needs the header, e.g. rate limiting,
    /// tick-window / staleness checks, replay diagnostics, or per-channel
    /// policy. The header is passed **by value (read-only)** — only the
    /// message data is `&mut` (mutate to rewrite, return `false` to drop) — so a
    /// validator can't accidentally rewrite the wire header.
    pub fn retain_received_messages(
        &mut self,
        mut keep: impl FnMut(ReceivedMessageHeader, &mut M) -> bool,
    ) {
        self.ready_mut().retain_mut(|received| {
            let header = ReceivedMessageHeader {
                remote_tick: received.remote_tick,
                channel_type_id: received.channel_type_id,
                message_id: received.message_id,
            };
            keep(header, &mut received.data)
        });
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
    /// Pushes `message` into the receiver of `user`, which goes on the entity first if it is not
    /// there yet.
    fn queue_received(
        commands: &ParallelCommands<'_, '_>,
        entity: Entity,
        user: EndpointUser,
        message: ReceivedMessage<M>,
    ) {
        commands.command_scope(|mut commands| {
            commands.queue(move |world: &mut World| {
                let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
                    return;
                };
                match user {
                    EndpointUser::Link => entity_mut
                        .entry::<Self>()
                        .or_default()
                        .get_mut()
                        .push_received(message),
                    EndpointUser::HostClient => entity_mut
                        .entry::<HostMessageReceiver<M>>()
                        .or_default()
                        .get_mut()
                        .push_received(message),
                }
            });
        });
    }

    /// The receiver of `user` among `receivers`.
    fn of_user<'a>(
        receivers: &'a mut FilteredEntityMut<'_, '_>,
        user: EndpointUser,
    ) -> Option<Mut<'a, Self>> {
        match user {
            EndpointUser::Link => receivers.get_mut::<Self>(),
            EndpointUser::HostClient => receivers
                .get_mut::<HostMessageReceiver<M>>()
                .map(|receiver| receiver.map_unchanged(|receiver| &mut **receiver)),
        }
    }

    /// Receive a single message of type `M` from the channel, into its user's receiver among
    /// `receivers`, or into one that goes on with the commands when absent.
    pub(crate) fn receive_message_typed(
        receivers: &mut FilteredEntityMut<'_, '_>,
        cx: MessageReceiveCx<'_, '_, '_>,
    ) -> Result<(), MessageError> {
        let MessageReceiveCx {
            commands,
            link: entity,
            user,
            reader,
            header:
                ReceivedMessageHeader {
                    remote_tick,
                    channel_type_id,
                    message_id,
                },
            channel_name,
            serialize_fns,
            entity_map,
            ..
        } = cx;
        let receiver = Self::of_user(receivers, user);
        let insert_receiver = receiver.is_none();
        let data = serialize_fns.deserialize::<M>(reader, entity_map)?;
        let message = ReceivedMessage {
            data,
            remote_tick,
            channel_type_id,
            message_id,
        };
        if let Some(mut receiver) = receiver {
            receiver.push_received(message);
        } else {
            Self::queue_received(commands, entity, user, message);
        }
        trace!(
            "Received message {:?} on channel {channel_type_id:?}",
            DebugName::type_name::<M>()
        );
        trace!(
            target: "campfire_net_debug::message",
            kind = "message_receive_typed",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            message_name = core::any::type_name::<M>(),
            channel = channel_name,
            remote_tick = remote_tick.0,
            message_id = ?message_id,
            insert_receiver,
            "deserialized message into receiver"
        );
        Ok(())
    }

    /// Clears `user`'s receiver among `receivers`, which the link lists.
    pub(crate) fn clear_typed(receivers: &mut FilteredEntityMut<'_, '_>, user: EndpointUser) {
        let mut receiver =
            Self::of_user(receivers, user).expect("a link holds each receiver it lists");
        if receiver.has_messages() {
            warn!(
                "Unhandled messages {:?}. Clearing to avoid accumulating messages...",
                DebugName::type_name::<M>()
            );
            receiver.ready_mut().clear();
        }
    }
}

/// The receiver of a host client's own side: what its server sends the host client.
///
/// A host client is also the server's link to it, whose [`MessageReceiver<M>`] holds what the
/// host client sends its server, so the host client's systems read this one. It has the API of
/// [`MessageReceiver<M>`].
#[derive(Debug, Component)]
#[require(LinkMessages)]
#[component(on_add = LinkMessages::list::<HostMessageReceiver<M>>, on_remove = LinkMessages::unlist::<HostMessageReceiver<M>>)]
pub struct HostMessageReceiver<M: Message>(MessageReceiver<M>);

impl<M: Message> Default for HostMessageReceiver<M> {
    fn default() -> Self {
        Self(MessageReceiver::default())
    }
}

impl<M: Message> Deref for HostMessageReceiver<M> {
    type Target = MessageReceiver<M>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<M: Message> DerefMut for HostMessageReceiver<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<M: Message> HostMessageReceiver<M> {}

impl<M: Message> ListedEndpoint for MessageReceiver<M> {
    const LIST: EndpointList = EndpointList::Receive(EndpointUser::Link);

    fn message_type_id() -> MessageTypeId {
        MessageTypeId::of::<M>()
    }
}

impl<M: Message> ListedEndpoint for HostMessageReceiver<M> {
    const LIST: EndpointList = EndpointList::Receive(EndpointUser::HostClient);

    fn message_type_id() -> MessageTypeId {
        MessageTypeId::of::<M>()
    }
}

#[cfg(test)]
mod tests;
