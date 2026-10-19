use crate::send_map_view::SendMapView;
use crate::serialize_fns::ErasedSerializeFns;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::world::FilteredEntityMut;
use bytes::Bytes;
use campfire_common::ErrorReport;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::Writer;
use campfire_net_channels::ChannelKind;
use campfire_net_channels::TransportError;
use campfire_net_connection::HostPayload;
use campfire_net_core::Direction;
use campfire_net_core::NetId;
use campfire_net_core::NetTick;
use campfire_net_transport::ChannelRegistry;
use campfire_net_transport::Transport;
use tracing::{error, trace};

pub type Priority = f32;

/// The values a sender holds until the frame sends them: the queue under
/// `MessageSender<M>` and
/// `EventSender<M>`, as under their host-side wrappers.
#[derive(Debug)]
pub(crate) struct SendQueue<M> {
    queued: Vec<Queued<M>>,
}

#[derive(Debug)]
struct Queued<M> {
    value: M,
    channel: ChannelKind,
    priority: Priority,
}

impl<M> Default for SendQueue<M> {
    fn default() -> Self {
        Self { queued: Vec::new() }
    }
}

/// Where a link's senders write: the writer their values are serialized with, the link's entity
/// map, the channel registry that names each channel's id, and the link itself.
#[derive(Debug)]
pub(crate) struct SendLink<'a, 'm> {
    pub(crate) writer: &'a mut Writer,
    pub(crate) entity_map: &'a SendMapView<'m>,
    pub(crate) channels: &'a ChannelRegistry,
    pub(crate) target: SendTarget<'a>,
}

/// What a link's serialized values go to.
#[derive(Debug)]
pub(crate) enum SendTarget<'a> {
    /// A remote link's transport, which packs them for its session.
    Transport(&'a mut Transport),
    /// A host client's buffer, which the other side of its entity reads from in the next
    /// receive, at the app's `tick`.
    Host {
        buffer: &'a mut Vec<HostPayload>,
        tick: NetTick,
        direction: Direction,
    },
}

impl SendTarget<'_> {
    /// A host client's buffer takes every message, as it skips the wire and its size limits.
    pub(crate) fn send(
        &mut self,
        channel: NetId,
        bytes: Bytes,
        priority: Priority,
    ) -> Result<(), TransportError> {
        match self {
            Self::Transport(transport) => transport.send(channel, bytes, priority),
            Self::Host {
                buffer,
                tick,
                direction,
            } => {
                buffer.push(HostPayload {
                    bytes,
                    channel,
                    tick: *tick,
                    direction: *direction,
                });
                Ok(())
            }
        }
    }
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

/// Why a link holds each sender its `MessageManager`
/// lists: the sender's hooks keep the list.
const LISTED_SENDER: &str = "a link holds each sender it lists";

/// A sender component, which holds a [`SendQueue<M>`].
pub(crate) trait QueuedSender<M>: Component<Mutability = Mutable> {
    fn queue_mut(&mut self) -> &mut SendQueue<M>;
}

impl<M: 'static> SendQueue<M> {
    pub(crate) fn push(&mut self, value: M, channel: ChannelKind, priority: Priority) {
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
    /// value whose serialization fails, or that is above its channel's size limit, is dropped
    /// alone: the ones queued behind it are sent.
    pub(crate) fn send<S: QueuedSender<M>>(
        senders: &mut FilteredEntityMut<'_, '_>,
        net_id: NetId,
        serialize_fns: &ErasedSerializeFns,
        link: &mut SendLink<'_, '_>,
    ) {
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
            let written = net_id
                .to_bytes(link.writer)
                .and_then(|()| serialize_fns.serialize::<M>(&value, link.writer, link.entity_map));
            if let Err(error) = written {
                link.writer.reset();
                error!(
                    ?error,
                    message = core::any::type_name::<M>(),
                    "dropped a message that could not be serialized"
                );
                continue;
            }
            let bytes = link.writer.take_written();
            trace!(
                target: "campfire_net_debug::message",
                kind = "message_send",
                schedule = "PostUpdate",
                sample_point = "PostUpdate",
                message_name = core::any::type_name::<M>(),
                message_net_id = net_id.get(),
                channel = link.channels.name(channel_id),
                bytes = bytes.len(),
                priority,
                "serialized message for transport"
            );
            match link.target.send(channel_id, bytes, priority) {
                Ok(()) => {}
                Err(error @ TransportError::MessageTooLarge { .. }) => {
                    error!(
                        ?error,
                        message = core::any::type_name::<M>(),
                        "dropped a message above its channel's size limit"
                    );
                }
                Err(error) => {
                    panic!(
                        "a message is sent only on a channel its transport sends on: {}",
                        ErrorReport::of(&error)
                    )
                }
            }
        }
    }

    /// Drops the queue of the `S` among `senders`, and returns how many values it held.
    pub(crate) fn discard<S: QueuedSender<M>>(senders: &mut FilteredEntityMut<'_, '_>) -> usize {
        let mut sender = senders.get_mut::<S>().expect(LISTED_SENDER);
        let queue = sender.queue_mut();
        let dropped = queue.queued.len();
        if dropped > 0 {
            queue.queued.clear();
        }
        dropped
    }
}
