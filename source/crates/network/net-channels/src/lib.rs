//! Messages over a link's packets, with no engine in it: channels and their delivery modes,
//! fragments, priority and the packing to the packet session's budget, the receive budget and the
//! tick, and the [`Channels`] of one link that run them.

#[cfg(feature = "internals")]
pub mod bench;
mod bench_counter;
mod candidate_order;
mod channel;
mod channel_message;
mod channel_mode;
mod channel_reader;
mod channel_receiver;
mod channel_sender;
mod channel_setup;
mod channel_slots;
mod channel_type_id;
mod error;
mod fragment_body;
mod fragment_count;
mod fragment_index;
mod fragment_receiver;
mod fragmented_message;
mod gap_store;
mod message_id;
mod message_ref;
mod packet;
mod packet_buffer_pool;
mod packet_builder;
mod packet_kind;
mod packet_message;
mod queued_message;
mod receive_budget;
mod reliable_send_state;
mod send_budget;
mod send_candidate;
mod send_gate;
mod send_message_key;
mod single_body;
mod tick_decoder;
mod tick_encoder;
mod unsettled_packets;

mod channels;

pub use crate::channel::Channel;
pub use crate::channel_message::ChannelMessage;
pub use crate::channel_mode::ChannelMode;
pub use crate::channel_reader::ChannelReader;
pub use crate::channel_receiver::ChannelReceiver;
pub use crate::channel_setup::ChannelSetup;
pub use crate::channel_type_id::ChannelTypeId;
pub use crate::channels::Channels;
pub use crate::error::{ReceiveError, SendRefusal};
pub use crate::message_id::MessageId;
pub use crate::send_gate::SendGate;

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::channels::internals::ChannelCounts;
    pub use crate::channels::internals::TransportBacklog;
    pub use crate::channels::internals::TransportCounts;
    pub use crate::packet::internals::fragment_len;
    pub use crate::packet_kind::internals::whole_messages;
}
