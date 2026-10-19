use crate::endpoint_side::EndpointSide;
use crate::receive_map_view::ReceiveMapView;
use crate::serialize_fns::ErasedSerializeFns;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::ParallelCommands;
use campfire_net_bytes::Reader;
use campfire_net_channels::ChannelKind;
use campfire_net_channels::MessageId;
use campfire_net_core::NetTick;
use campfire_net_core::PeerId;

/// What a typed receive fn reads one received message with, and where it delivers it.
pub(crate) struct MessageReceiveContext<'a, 'w, 's> {
    pub(crate) commands: &'a ParallelCommands<'w, 's>,
    /// The link the message arrived on.
    pub(crate) link: Entity,
    /// The side of the link whose receiver the message lands on.
    pub(crate) side: EndpointSide,
    pub(crate) peer: PeerId,
    pub(crate) reader: &'a mut Reader,
    pub(crate) header: ReceivedMessageHeader,
    pub(crate) channel_name: &'static str,
    pub(crate) serialize_fns: &'a ErasedSerializeFns,
    pub(crate) entity_map: &'a ReceiveMapView<'a>,
}

impl core::fmt::Debug for MessageReceiveContext<'_, '_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MessageReceiveContext")
            .field("link", &self.link)
            .field("side", &self.side)
            .field("peer", &self.peer)
            .field("header", &self.header)
            .field("channel_name", &self.channel_name)
            .finish_non_exhaustive()
    }
}

/// The read-only header of each received message, handed to
/// [`MessageReceiver::retain_received_messages`] validators alongside `&mut`
/// access to the message data.
#[derive(Debug, Clone, Copy)]
pub struct ReceivedMessageHeader {
    /// NetTick on the remote peer when the message was sent.
    pub remote_tick: NetTick,
    /// Channel the message was sent on.
    pub channel_kind: ChannelKind,
    /// MessageId of the message, if the channel assigns one.
    pub message_id: Option<MessageId>,
}
