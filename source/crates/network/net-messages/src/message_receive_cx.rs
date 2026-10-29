use bevy_ecs::entity::Entity;
use bevy_ecs::system::Commands;
use campfire_net_bytes::Reader;
use campfire_net_channels::ChannelTypeId;
use campfire_net_channels::MessageId;
use campfire_net_core::NetTick;
use campfire_net_core::PeerId;

use crate::receive_map_view::ReceiveMapView;
use crate::serialize_fns::ErasedSerializeFns;

/// What a typed receive fn reads one received message with, and where it delivers it.
pub(crate) struct MessageReceiveCx<'a, 'w, 's> {
    pub(crate) commands: &'a mut Commands<'w, 's>,
    /// The link the message arrived on.
    pub(crate) link: Entity,
    pub(crate) peer: PeerId,
    pub(crate) reader: &'a mut Reader,
    pub(crate) header: ReceivedMessageHeader,
    pub(crate) channel_name: &'static str,
    pub(crate) serialize_fns: &'a ErasedSerializeFns,
    pub(crate) entity_map: &'a ReceiveMapView<'a>,
}

impl core::fmt::Debug for MessageReceiveCx<'_, '_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MessageReceiveCx")
            .field("link", &self.link)
            .field("peer", &self.peer)
            .field("header", &self.header)
            .field("channel_name", &self.channel_name)
            .finish_non_exhaustive()
    }
}

/// The read-only header of each received message.
#[derive(Debug, Clone, Copy)]
pub struct ReceivedMessageHeader {
    /// NetTick on the remote peer when the message was sent.
    pub remote_tick: NetTick,
    /// Channel the message was sent on.
    pub channel_type_id: ChannelTypeId,
    /// MessageId of the message, if the channel assigns one.
    pub message_id: Option<MessageId>,
}
