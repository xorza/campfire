use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::world::FilteredEntityMut;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::Writer;
use campfire_net_channels::ChannelTypeId;
use campfire_net_core::NetId;
use campfire_net_transport::ChannelRegistry;
use campfire_net_transport::Transport;
use tracing::trace;

use crate::send_map_view::SendMapView;
use crate::serialize_fns::ErasedSerializeFns;

/// The values a sender holds until the frame sends them: the queue under
/// `MessageSender<M>` and
/// `EventSender<M>`.
#[derive(Debug)]
pub(crate) struct SendQueue<M> {
    queued: Vec<Queued<M>>,
}

#[derive(Debug)]
struct Queued<M> {
    value: M,
    channel: ChannelTypeId,
    priority: f32,
}

impl<M> Default for SendQueue<M> {
    fn default() -> Self {
        Self { queued: Vec::new() }
    }
}

/// Where a link's senders write: the writer their values are serialized with, the link's entity
/// map, the channel registry that names each channel's id, and the link's transport, which packs
/// them for its packet session.
#[derive(Debug)]
pub(crate) struct SendLink<'a, 'm> {
    pub(crate) writer: &'a mut Writer,
    pub(crate) entity_map: &'a SendMapView<'m>,
    pub(crate) channels: &'a ChannelRegistry,
    pub(crate) transport: &'a mut Transport,
}

/// Serializes the queue of the sender the fn was built for, among `senders`, into its link.
pub(crate) type SendFn = fn(
    senders: &mut FilteredEntityMut<'_, '_>,
    net_id: NetId,
    serialize_fns: &ErasedSerializeFns,
    link: &mut SendLink<'_, '_>,
);

/// Drops the queue of the sender the fn was built for, among `senders`, whose link sends nothing,
/// and returns how many it dropped.
pub(crate) type DiscardSendFn = fn(senders: &mut FilteredEntityMut<'_, '_>) -> usize;

/// Why a link holds each sender its `LinkMessages`
/// lists: the sender's hooks keep the list.
const LISTED_SENDER: &str = "a link holds each sender it lists";

/// A sender component, which holds a [`SendQueue<M>`].
pub(crate) trait QueuedSender<M>: Component<Mutability = Mutable> {
    fn queue(&self) -> &SendQueue<M>;

    fn queue_mut(&mut self) -> &mut SendQueue<M>;
}

impl<M: 'static> SendQueue<M> {
    pub(crate) fn push(&mut self, value: M, channel: ChannelTypeId, priority: f32) {
        self.queued.push(Queued {
            value,
            channel,
            priority,
        });
    }

    #[cfg(test)]
    pub(crate) const fn len(&self) -> usize {
        self.queued.len()
    }

    /// Serializes each value queued in the `S` among `senders`, after its net id, into `link`. A
    /// sender with nothing queued is read alone, so it stays unchanged.
    pub(crate) fn send<S: QueuedSender<M>>(
        senders: &mut FilteredEntityMut<'_, '_>,
        net_id: NetId,
        serialize_fns: &ErasedSerializeFns,
        link: &mut SendLink<'_, '_>,
    ) {
        if senders
            .get::<S>()
            .expect(LISTED_SENDER)
            .queue()
            .queued
            .is_empty()
        {
            return;
        }
        let mut sender = senders.get_mut::<S>().expect(LISTED_SENDER);
        let queue = sender.queue_mut();
        for Queued {
            value,
            channel,
            priority,
        } in queue.queued.drain(..)
        {
            let channel_id = link
                .channels
                .net_id(channel)
                .expect("a message is sent on a registered channel");
            net_id.to_bytes(link.writer);
            serialize_fns.serialize::<M>(&value, link.writer, link.entity_map);
            let bytes = link.writer.take_written();
            trace!(
                target: "campfire_net_debug::message",
                kind = "message_send",
                message_name = core::any::type_name::<M>(),
                message_net_id = net_id.get(),
                channel = link.channels.name(channel_id),
                bytes = bytes.len(),
                priority,
                "serialized message for transport"
            );
            link.transport.send(channel_id, bytes, priority);
        }
    }

    /// Drops the queue of the `S` among `senders`, and returns how many values it held; a sender
    /// with nothing queued is read alone, so it stays unchanged.
    pub(crate) fn discard<S: QueuedSender<M>>(senders: &mut FilteredEntityMut<'_, '_>) -> usize {
        let dropped = senders
            .get::<S>()
            .expect(LISTED_SENDER)
            .queue()
            .queued
            .len();
        if dropped > 0 {
            let mut sender = senders.get_mut::<S>().expect(LISTED_SENDER);
            sender.queue_mut().queued.clear();
        }
        dropped
    }
}
