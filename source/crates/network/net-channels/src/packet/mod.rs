//! A packet: the transport's payload of one session packet, filled with the messages of its
//! channels in priority order.
//!
//! Its bytes start after the session's headroom, where the session writes its header in place,
//! with the tick; the messages follow in batches, or one fragment and then batches. A message
//! larger than a packet's payload is split into `FragmentData`.

use crate::packet_kind::PacketKind;
use crate::packet_message::PacketMessage;
use crate::tick_encoder::LONG_TICK_LEN;
use bytes::BytesMut;
use campfire_net_bytes::varint;
use campfire_net_payload::{DEFAULT_MTU, OutgoingPayload};

/// The most fragments a message splits into: the largest count a two-byte varint holds, so the
/// fragment id and count reserve two bytes each.
pub(crate) const MAX_FRAGMENTS: usize = 16_383;

const MAX_FRAGMENT_CHANNEL_ID_BYTES: usize = varint::len(u16::MAX as u64);
const MAX_FRAGMENT_ID_BYTES: usize = varint::len(MAX_FRAGMENTS as u64 - 1);
const MAX_FRAGMENT_COUNT_BYTES: usize = varint::len(MAX_FRAGMENTS as u64);
const INITIAL_FRAGMENT_COMPRESSION_BYTES: usize = 1;
const FIXED_FRAGMENT_METADATA_BYTES: usize = MAX_FRAGMENT_CHANNEL_ID_BYTES
    + 2 // MessageId
    + MAX_FRAGMENT_ID_BYTES
    + MAX_FRAGMENT_COUNT_BYTES
    + INITIAL_FRAGMENT_COMPRESSION_BYTES;
const FIXED_FRAGMENT_PACKET_BYTES: usize = LONG_TICK_LEN + FIXED_FRAGMENT_METADATA_BYTES;

/// The smallest payload which can carry a fragment packet with one byte of fragment.
///
/// It reserves the long tick and the largest supported channel, message, and fragment identifier
/// encodings.
pub(crate) const MIN_FRAGMENT_PAYLOAD: usize = FIXED_FRAGMENT_PACKET_BYTES
    + 1 // encoded fragment byte length
    + 1; // payload

/// Returns the fragment size a sender uses on a link whose packets carry at most `max_payload`
/// bytes.
///
/// Only the sender's own link sets it: the receiver takes each message's fragment size from that
/// message's fragments, so the two peers' links do not have to agree. Reserving the long tick and
/// the largest identifier encodings ensures every fragment produced at this size fits even for
/// long-running sessions and very large logical messages.
pub const fn fragment_size_for_max_payload(max_payload: usize) -> Option<usize> {
    let Some(available) = max_payload.checked_sub(FIXED_FRAGMENT_PACKET_BYTES) else {
        return None;
    };

    // `Bytes` encodes its payload length as a varint. Start at all remaining space and find the
    // largest payload whose own length prefix plus bytes fit in that space.
    let mut fragment_size = available;
    while fragment_size > 0 {
        let length_prefix_bytes = varint::len(fragment_size as u64);
        if fragment_size <= available.saturating_sub(length_prefix_bytes) {
            return Some(fragment_size);
        }
        fragment_size -= 1;
    }
    None
}

/// The largest payload of a datagram of [`DEFAULT_MTU`] bytes under the session's longest header
/// and its tag, which a transport assumes until its link names its own.
pub(crate) const DEFAULT_MAX_PAYLOAD: usize = OutgoingPayload::max_payload(DEFAULT_MTU);

/// The fragment size of a link of [`DEFAULT_MAX_PAYLOAD`], until the link names its own.
pub(crate) const FRAGMENT_SIZE: usize = match fragment_size_for_max_payload(DEFAULT_MAX_PAYLOAD) {
    Some(fragment_size) => fragment_size,
    None => panic!("the default payload cannot hold transport fragment metadata"),
};

/// A packet the builder staged: its payload after the session's headroom, its kind and the
/// messages it carries.
#[derive(Debug)]
pub(crate) struct Packet {
    pub(crate) payload: BytesMut,
    pub(crate) kind: PacketKind,
    /// One entry per staged message.
    pub(crate) messages: Vec<PacketMessage>,
    pub(crate) compression: Option<PacketCompressionInfo>,
    /// The body compressed, but not to fewer bytes or not within its room, so it goes
    /// uncompressed.
    pub(crate) compression_abandoned: bool,
}

impl Packet {
    /// Transfers the payload to the session, which writes its header into the headroom and its
    /// tag past the end: the payload and its tailroom, split off the rest of the buffer, which
    /// drops.
    pub(crate) fn take_send_payload(&mut self) -> BytesMut {
        let mut payload = core::mem::take(&mut self.payload);
        let len = payload.len();
        payload.resize(len + OutgoingPayload::TAILROOM, 0);
        let mut handed = payload.split();
        handed.truncate(len);
        handed
    }

    /// The bytes the packet's payload takes on the wire, after the session's headroom.
    pub(crate) fn len(&self) -> usize {
        self.payload.len() - OutgoingPayload::HEADROOM
    }

    pub(crate) const fn num_messages(&self) -> usize {
        self.messages.len()
    }

    pub(crate) fn record_message(&mut self, message: PacketMessage) {
        self.messages.push(message);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PacketCompressionInfo {
    pub(crate) original_len: usize,
    pub(crate) compressed_len: usize,
}

#[cfg(test)]
mod tests;
