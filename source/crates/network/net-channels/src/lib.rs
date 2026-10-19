//! Messages over a link's packets, with no engine in it: channels and their delivery modes,
//! fragments, priority and the packing to the session's budget, compression, the receive budget
//! and the tick, and the [`Channels`] of one link that run them.

#[cfg(feature = "internals")]
pub mod bench;
mod bench_counter;
mod channel;
mod channel_config;
mod channel_kind;
mod channel_message;
mod channel_mode;
mod channel_receive_error;
mod channel_receiver;
mod channel_sender;
mod channel_slots;
mod compression;
mod compression_algorithm;
mod compression_config;
mod compression_scratch;
mod fragment_compression;
mod fragment_count;
mod fragment_data;
mod fragment_index;
mod fragment_receiver;
mod fragmented_message;
#[cfg(feature = "internals")]
pub mod internals;
mod message_data;
mod message_id;
mod message_ref;
pub mod packet;
mod packet_buffer_pool;
mod packet_builder;
mod packet_error;
mod packet_kind;
mod packet_message;
mod priority_manager;
pub mod receive_budget;
mod receive_message;
mod reliable_send_state;
mod send_candidate;
mod send_message_key;
mod sent_packets;
mod single_data;
mod tick_decoder;
mod tick_encoder;
mod transport_error;

mod channels;

pub use crate::channel::Channel;
pub use crate::channel_config::ChannelConfig;
pub use crate::channel_kind::ChannelKind;
pub use crate::channel_message::ChannelMessage;
pub use crate::channel_mode::ChannelMode;
pub use crate::channel_receive_error::ChannelReceiveError;
pub use crate::channel_receiver::ChannelReceiver;
pub use crate::channel_sender::ChannelSender;
pub use crate::channels::Channels;
pub use crate::channels::RefusedMessage;
pub use crate::compression_algorithm::CompressionAlgorithm;
pub use crate::compression_config::CompressionConfig;
pub use crate::message_id::MessageId;
pub use crate::packet_error::PacketError;
pub use crate::packet_kind::PacketKind;
pub use crate::transport_error::TransportError;
