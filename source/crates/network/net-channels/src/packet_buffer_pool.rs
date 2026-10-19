use crate::packet::Packet;
use crate::packet_message::PacketMessage;
use bytes::BytesMut;
use campfire_net_common::Arena;

/// The packets of a payload chunk: a pass of a full window fills about one, and the arena keeps
/// three more while the session holds their datagrams, 64 payloads in all.
const PACKETS_PER_CHUNK: usize = 16;

/// The chunks besides the current one the payload arena keeps for reuse.
const SPARE_CHUNKS: usize = 3;

/// Reusable packet-builder storage.
///
/// The payloads are views of an [`Arena`], so a flush's packets share a few allocations, each
/// reused once the session dropped the datagrams of its views. This also keeps the list of a
/// packet's messages: a flush commits each packet before it builds the next, so one list serves
/// every packet and keeps the capacity the largest one gave it.
#[derive(Debug)]
pub(crate) struct PacketBufferPool {
    pub(crate) payloads: Arena,
    /// The bytes of a payload: the session's headroom, the largest payload, and its tailroom.
    payload_capacity: usize,
    packet_messages: Vec<PacketMessage>,
}

impl PacketBufferPool {
    pub(crate) fn new(payload_capacity: usize) -> Self {
        Self {
            payloads: Arena::new(payload_capacity * PACKETS_PER_CHUNK, SPARE_CHUNKS),
            payload_capacity,
            packet_messages: Vec::new(),
        }
    }

    /// An empty payload with room for a packet.
    pub(crate) fn take_payload(&mut self) -> BytesMut {
        self.payloads.take(self.payload_capacity)
    }

    /// Sets the bytes of a payload, as a link names its largest packet.
    pub(crate) fn set_payload_capacity(&mut self, payload_capacity: usize) {
        self.payload_capacity = payload_capacity;
        self.payloads
            .set_chunk(payload_capacity * PACKETS_PER_CHUNK);
    }

    pub(crate) fn take_packet_messages(&mut self) -> Vec<PacketMessage> {
        core::mem::take(&mut self.packet_messages)
    }

    /// Takes back the message list of a staged packet that was not handed to `Link`; its payload
    /// goes back to its chunk as it drops.
    pub(crate) fn recycle_packet(&mut self, packet: Packet) {
        self.recycle_packet_messages(packet.messages);
    }

    /// Takes back a packet's message list, the larger of it and the kept one.
    pub(crate) fn recycle_packet_messages(&mut self, mut messages: Vec<PacketMessage>) {
        if messages.capacity() > self.packet_messages.capacity() {
            messages.clear();
            self.packet_messages = messages;
        }
    }
}
