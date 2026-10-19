use bytes::Bytes;
use campfire_net_core::NetTick;
use campfire_net_payload::DEFAULT_MTU;

use crate::channel::Channel;
use crate::channel_kind::ChannelKind;
#[cfg(feature = "compression_lz4")]
use crate::compression::tests::decompress;
use crate::fragmented_message::FragmentedMessage;
use crate::message_id::MessageId;
use crate::packet::FRAGMENT_SIZE;
use crate::send_candidate::{CandidateRank, CandidateSink};
use crate::tick_encoder::{LONG_TICK_LEN, TickEncoder};
use bevy_platform::collections::HashMap;

use super::*;

#[derive(Debug)]
struct Channel1;
impl Channel for Channel1 {}

#[derive(Debug)]
struct Channel2;
impl Channel for Channel2 {}

#[derive(Debug)]
struct Channel3;
impl Channel for Channel3 {}

/// The wire id of each test channel, as a registry gives them, in the order of their names.
fn net_id(kind: ChannelKind) -> NetId {
    let kinds = [
        ChannelKind::of::<Channel1>(),
        ChannelKind::of::<Channel2>(),
        ChannelKind::of::<Channel3>(),
    ];
    let index = kinds.iter().position(|known| *known == kind).unwrap();
    NetId::new(u16::try_from(index).unwrap())
}

/// Candidates with the messages they name, staged without channels: none is committed, so a
/// fragmented message's fragments all stay due.
#[derive(Debug, Default)]
struct Staged {
    candidates: Vec<SendCandidate>,
    singles: HashMap<(NetId, u32), SingleData>,
    fragmented: HashMap<(NetId, u32), FragmentedMessage>,
}

impl Staged {
    fn sink(&mut self, channel_id: NetId) -> CandidateSink<'_> {
        CandidateSink {
            channel_id,
            channel_priority: 1.0,
            candidates: &mut self.candidates,
        }
    }

    /// Stages one single of priority 1, ordered by its queue index.
    fn single(&mut self, (_, channel_id): (ChannelKind, NetId), index: u32, bytes: Bytes) {
        let single = SingleData::new(None, bytes);
        let rank = CandidateRank {
            priority: 1.0,
            send_order: index,
        };
        self.sink(channel_id).push(
            SendMessageKey::UnreliableSingle(index),
            rank,
            MessageRef::Single(&single),
        );
        self.singles.insert((channel_id, index), single);
    }

    /// Stages one fragmented message of priority 1 at the queue index `index`.
    fn fragmented(
        &mut self,
        (_, channel_id): (ChannelKind, NetId),
        index: u32,
        message: FragmentedMessage,
    ) {
        let rank = CandidateRank {
            priority: 1.0,
            send_order: index,
        };
        self.sink(channel_id).push(
            SendMessageKey::UnreliableFragment(index, 0),
            rank,
            MessageRef::Fragment(message.fragment(0)),
        );
        self.fragmented.insert((channel_id, index), message);
    }

    /// The source of a flush whose packets carry a long tick, as before the peer read one.
    fn source(&self, compression: CompressionConfig, max_payload: usize) -> PacketSource<'_, Self> {
        PacketSource {
            tick: TickEncoder::default().encode(NetTick(0)),
            candidates: self.candidates.as_slice(),
            messages: self,
            compression,
            max_payload,
        }
    }
}

impl CandidateMessages for Staged {
    fn message(&self, channel_id: NetId, key: SendMessageKey) -> MessageRef<'_> {
        match key {
            SendMessageKey::UnreliableSingle(index) => {
                MessageRef::Single(&self.singles[&(channel_id, index)])
            }
            SendMessageKey::UnreliableFragment(index, fragment) => {
                MessageRef::Fragment(self.fragmented[&(channel_id, index)].fragment(fragment))
            }
            SendMessageKey::ReliableSingle(_) | SendMessageKey::ReliableFragment(..) => {
                unreachable!("the stage holds unreliable messages")
            }
        }
    }

    fn next_due(&self, channel_id: NetId, key: SendMessageKey) -> Option<SendMessageKey> {
        let SendMessageKey::UnreliableFragment(index, fragment) = key else {
            unreachable!("only a fragment has a next fragment")
        };
        self.fragmented[&(channel_id, index)]
            .next_due(fragment + 1)
            .map(|next| key.with_fragment(next))
    }
}

/// A message of `len` bytes, split at `fragment_size`.
fn split(message_id: MessageId, len: usize, fragment_size: usize) -> FragmentedMessage {
    FragmentedMessage::split(
        message_id,
        Bytes::from(vec![9u8; len]),
        fragment_size,
        CompressionConfig::DISABLED,
        &mut CompressionScratch::default(),
    )
}

fn build_staged_packets(
    builder: &mut PacketBuilder,
    staged: &Staged,
    compression: CompressionConfig,
) -> Result<Vec<Packet>, PacketError> {
    let mut cursor = CandidateCursor::default();
    let mut packets = Vec::new();
    let mut compression_scratch = CompressionScratch::default();
    while let Some(packet) = builder.build_next_packet(
        &staged.source(compression, DEFAULT_MAX_PAYLOAD),
        &mut cursor,
        &mut compression_scratch,
    )? {
        packets.push(packet);
    }
    Ok(packets)
}

/// The bytes before a test packet's body: the session's headroom and a long tick.
#[cfg(feature = "compression_lz4")]
const HEADER_LEN: usize = OutgoingPayload::HEADROOM + LONG_TICK_LEN;

/// No message of the test channels carries an id.
fn no_ids(_: NetId) -> bool {
    false
}

#[cfg(feature = "compression_lz4")]
fn decompress_packet_for_test(mut packet: Packet) -> Result<Packet, PacketError> {
    let decompressed_payload = decompress(&packet.payload[HEADER_LEN..], CompressionConfig::LZ4)?;
    packet.kind = packet.kind.uncompressed_variant();
    packet.payload.truncate(HEADER_LEN);
    packet.payload.extend_from_slice(&decompressed_payload);
    Ok(packet)
}

#[cfg(feature = "compression_lz4")]
fn random_payload(len: usize, message_index: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15u64 ^ message_index as u64;
    let mut payload = Vec::with_capacity(len);
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        payload.push((state & 0xff) as u8);
    }
    payload
}

/// A flush gives a packet's message list back before the next packet takes it, so one list of
/// any size keeps its capacity: 1,000 messages' worth comes back, and a smaller list does not
/// replace it.
#[test]
fn the_packet_message_list_keeps_its_capacity() {
    let mut pool = PacketBufferPool::new(DEFAULT_MTU);

    pool.recycle_packet_messages(Vec::with_capacity(1_000));
    let messages = pool.take_packet_messages();
    assert_eq!(messages.capacity(), 1_000);
    pool.recycle_packet_messages(messages);
    pool.recycle_packet_messages(Vec::with_capacity(10));
    assert_eq!(pool.take_packet_messages().capacity(), 1_000);
}

/// A flush's payloads are views of a few chunks: 16 alive fill the first chunk, the next 16 take
/// the spare made with it, and once the first 16 drop, their chunk serves the next 16, with no
/// allocation past the first two.
#[test]
fn payloads_share_chunks_that_come_back_once_they_drop() {
    let mut pool = PacketBufferPool::new(DEFAULT_MTU);
    let first: Vec<BytesMut> = (0..16).map(|_| pool.take_payload()).collect();
    assert_eq!(first[0].capacity(), DEFAULT_MTU);
    assert_eq!(
        first[1].as_ptr().addr(),
        first[0].as_ptr().addr() + DEFAULT_MTU
    );
    let second: Vec<BytesMut> = (0..16).map(|_| pool.take_payload()).collect();
    assert_eq!(pool.payloads.allocations(), 2);
    let chunk_0 = first[0].as_ptr();
    drop(first);
    let third: Vec<BytesMut> = (0..16).map(|_| pool.take_payload()).collect();
    assert_eq!(third[0].as_ptr(), chunk_0);
    assert_eq!(pool.payloads.allocations(), 2);
    drop((second, third));
}

/// The handed payload keeps the tailroom the seal appends its tag in, and leaves the packet none.
#[test]
fn send_handoff_keeps_the_tailroom() {
    let mut payload = BytesMut::with_capacity(DEFAULT_MTU * 2);
    payload.extend_from_slice(&std::vec![0; DEFAULT_MTU]);
    let mut packet = Packet {
        payload,
        kind: PacketKind::Data,
        messages: Vec::new(),
        compression: None,
        compression_abandoned: false,
    };

    let bytes = packet.take_send_payload();

    assert_eq!(bytes.len(), DEFAULT_MTU);
    assert_eq!(bytes.capacity(), DEFAULT_MTU + OutgoingPayload::TAILROOM);
    assert_eq!(packet.payload.capacity(), 0);
}

#[test]
fn staged_builder_packs_singles_across_channels() -> Result<(), PacketError> {
    let channels = [
        ChannelKind::of::<Channel1>(),
        ChannelKind::of::<Channel2>(),
        ChannelKind::of::<Channel3>(),
    ]
    .map(|kind| {
        let id = net_id(kind);
        (kind, id)
    });
    let mut staged = Staged::default();
    for (index, (channel, bytes)) in [
        (channels[0], Bytes::from_static(b"one")),
        (channels[1], Bytes::from_static(b"two")),
        (channels[1], Bytes::from_static(b"three")),
        (channels[2], Bytes::from_static(b"four")),
    ]
    .into_iter()
    .enumerate()
    {
        staged.single(channel, u32::try_from(index).unwrap(), bytes);
    }

    let mut packets = build_staged_packets(
        &mut PacketBuilder::default(),
        &staged,
        CompressionConfig::DISABLED,
    )?;
    assert_eq!(packets.len(), 1);
    let contents = packets.pop().unwrap().parse_packet_payload(no_ids)?;
    assert_eq!(contents[&channels[0].1], [Bytes::from_static(b"one")]);
    assert_eq!(
        contents[&channels[1].1],
        [Bytes::from_static(b"two"), Bytes::from_static(b"three")]
    );
    assert_eq!(contents[&channels[2].1], [Bytes::from_static(b"four")]);
    Ok(())
}

#[test]
fn staged_builder_splits_singles_at_mtu() -> Result<(), PacketError> {
    let channels = [
        ChannelKind::of::<Channel1>(),
        ChannelKind::of::<Channel2>(),
        ChannelKind::of::<Channel3>(),
    ]
    .map(|kind| {
        let id = net_id(kind);
        (kind, id)
    });
    let mut staged = Staged::default();
    for (index, channel) in [channels[0], channels[1], channels[1], channels[2]]
        .into_iter()
        .enumerate()
    {
        staged.single(
            channel,
            u32::try_from(index).unwrap(),
            Bytes::from(vec![index as u8; 500]),
        );
    }

    let packets = build_staged_packets(
        &mut PacketBuilder::default(),
        &staged,
        CompressionConfig::DISABLED,
    )?;
    assert_eq!(packets.len(), 2);
    assert!(
        packets
            .iter()
            .all(|packet| packet.len() <= DEFAULT_MAX_PAYLOAD)
    );
    let message_count = packets
        .into_iter()
        .map(|packet| packet.parse_packet_payload(no_ids))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|contents| contents.values().map(Vec::len).sum::<usize>())
        .sum::<usize>();
    assert_eq!(message_count, 4);
    Ok(())
}

#[test]
fn staged_fragment_packets_respect_worst_case_mtu_overhead() -> Result<(), PacketError> {
    let channel_kind = ChannelKind::of::<Channel1>();
    let channel_id = 64;
    let message = split(MessageId(1024), FRAGMENT_SIZE * 64 + 1, FRAGMENT_SIZE);
    let expected_packet_count = usize::from(message.num_fragments());
    assert_eq!(expected_packet_count, 65);
    let mut staged = Staged::default();
    staged.fragmented((channel_kind, NetId::new(channel_id)), 0, message);

    let packets = build_staged_packets(
        &mut PacketBuilder::default(),
        &staged,
        CompressionConfig::DISABLED,
    )?;
    assert_eq!(packets.len(), expected_packet_count);
    assert!(
        packets
            .iter()
            .all(|packet| packet.len() <= DEFAULT_MAX_PAYLOAD)
    );
    Ok(())
}

#[cfg(feature = "compression_lz4")]
#[test]
fn staged_compression_can_pack_beyond_uncompressed_mtu() -> Result<(), PacketError> {
    let channel_kind = ChannelKind::of::<Channel1>();
    let channel_id = net_id(channel_kind);
    let mut staged = Staged::default();
    for index in 0..128 {
        staged.single(
            (channel_kind, channel_id),
            index,
            Bytes::from(vec![7u8; 64]),
        );
    }
    let compression = CompressionConfig {
        min_payload_size: 0,
        ..CompressionConfig::LZ4
    };

    let mut packets = build_staged_packets(&mut PacketBuilder::default(), &staged, compression)?;
    assert_eq!(packets.len(), 1);
    let packet = packets.pop().unwrap();
    assert!(packet.len() <= DEFAULT_MAX_PAYLOAD);
    // The payload passed its largest size only in the builder's staging buffer: the pooled
    // buffer the packet leaves in keeps the pool's capacity, so the pool takes it back.
    assert_eq!(
        packet.payload.capacity(),
        PacketBuilder::buffer_capacity(DEFAULT_MAX_PAYLOAD)
    );
    assert_eq!(packet.kind, PacketKind::DataCompressed);
    let contents = decompress_packet_for_test(packet)?.parse_packet_payload(no_ids)?;
    assert_eq!(contents[&channel_id].len(), 128);
    Ok(())
}

#[cfg(feature = "compression_lz4")]
#[test]
fn staged_compression_preserves_mtu_for_incompressible_data() -> Result<(), PacketError> {
    let channel_kind = ChannelKind::of::<Channel1>();
    let channel_id = net_id(channel_kind);
    let mut staged = Staged::default();
    for index in 0..128 {
        staged.single(
            (channel_kind, channel_id),
            index,
            random_payload(128, usize::try_from(index).unwrap()).into(),
        );
    }
    let compression = CompressionConfig {
        min_payload_size: 0,
        ..CompressionConfig::LZ4
    };

    let packets = build_staged_packets(&mut PacketBuilder::default(), &staged, compression)?;
    let mut message_count = 0;
    for packet in packets {
        assert!(packet.len() <= DEFAULT_MAX_PAYLOAD);
        let packet = if packet.kind.is_compressed() {
            decompress_packet_for_test(packet)?
        } else {
            packet
        };
        message_count += packet
            .parse_packet_payload(no_ids)?
            .get(&channel_id)
            .map_or(0, Vec::len);
    }
    assert_eq!(message_count, 128);
    Ok(())
}

/// Two messages of two fragments of 900 bytes each go one fragment to a packet, and the
/// packet of each one's last fragment fills with two of the five singles of 100 bytes; the
/// fifth single goes alone.
#[test]
fn staged_builder_fills_each_final_fragment_with_singles() -> Result<(), PacketError> {
    let channel_kind = ChannelKind::of::<Channel1>();
    let channel_id = net_id(channel_kind);
    let mut staged = Staged::default();

    for index in 0..2 {
        staged.fragmented(
            (channel_kind, channel_id),
            index,
            split(MessageId(u16::try_from(index).unwrap()), 1800, 900),
        );
    }
    for index in 0..5 {
        staged.single(
            (channel_kind, channel_id),
            index,
            Bytes::from(vec![index as u8; 100]),
        );
    }

    let packets = build_staged_packets(
        &mut PacketBuilder::default(),
        &staged,
        CompressionConfig::DISABLED,
    )?;

    let keys = packets
        .iter()
        .map(|packet| {
            packet
                .messages
                .iter()
                .map(|message| message.commit.key)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let single = SendMessageKey::UnreliableSingle;
    let fragment = SendMessageKey::UnreliableFragment;
    assert_eq!(
        keys,
        [
            vec![fragment(0, 0)],
            vec![fragment(0, 1), single(0), single(1)],
            vec![fragment(1, 0)],
            vec![fragment(1, 1), single(2), single(3)],
            vec![single(4)],
        ]
    );
    Ok(())
}

#[test]
fn staged_builder_splits_channel_batches_at_u8_limit() -> Result<(), PacketError> {
    let channel_kind = ChannelKind::of::<Channel1>();
    let channel_id = net_id(channel_kind);
    let mut staged = Staged::default();
    for index in 0..300 {
        staged.single((channel_kind, channel_id), index, Bytes::new());
    }

    let mut builder = PacketBuilder::default();
    let mut cursor = CandidateCursor::default();
    let mut compression_scratch = CompressionScratch::default();
    let packet = builder
        .build_next_packet(
            &staged.source(CompressionConfig::DISABLED, DEFAULT_MAX_PAYLOAD),
            &mut cursor,
            &mut compression_scratch,
        )?
        .unwrap();
    assert_eq!(packet.messages.len(), 300);
    let contents = packet.parse_packet_payload(no_ids)?;
    assert_eq!(contents[&channel_id].len(), 300);
    assert!(
        builder
            .build_next_packet(
                &staged.source(CompressionConfig::DISABLED, DEFAULT_MAX_PAYLOAD),
                &mut cursor,
                &mut compression_scratch
            )?
            .is_none()
    );
    Ok(())
}

/// A message's last fragment leaves a tail of 1 byte in a packet of 128, under the 4 that a
/// single takes on channel 64, whose id takes 2: the single does not fit, the append rolls back,
/// and the single goes in a packet of its own. Fragments of 115 bytes are the largest whose
/// first fragment, with its compression byte, fills the 128; the last, of 115 bytes too, takes
/// 127 without it.
#[test]
fn final_fragment_tail_uses_actual_single_channel_width_and_rolls_back() -> Result<(), PacketError>
{
    let max_payload = 128;
    let fragment_size = 115;
    let fragment_channel_id = NetId::new(0);
    let single_channel_id = NetId::new(64);
    assert!(single_channel_id.bytes_len() > fragment_channel_id.bytes_len());

    let single = SingleData::new(None, Bytes::new());
    let single_wire_len = single_channel_id.bytes_len() + 1 + single.bytes_len();
    assert_eq!(single_wire_len, 4);
    let packet_len = |message: &FragmentedMessage, index| {
        LONG_TICK_LEN + fragment_channel_id.bytes_len() + message.fragment(index).bytes_len()
    };
    let last_len = (1..=fragment_size)
        .rev()
        .find(|last_len| {
            let message = split(MessageId(0), fragment_size + last_len, fragment_size);
            let len = packet_len(&message, 1);
            len <= max_payload && max_payload - len < single_wire_len
        })
        .expect("test MTU should leave a tail smaller than the single batch");
    let message = split(MessageId(0), fragment_size + last_len, fragment_size);
    assert_eq!(packet_len(&message, 0), max_payload);
    assert_eq!(packet_len(&message, 1), max_payload - 1);

    let mut staged = Staged::default();
    staged.fragmented(
        (ChannelKind::of::<Channel1>(), fragment_channel_id),
        0,
        message,
    );
    staged.single(
        (ChannelKind::of::<Channel2>(), single_channel_id),
        0,
        Bytes::new(),
    );

    let mut builder = PacketBuilder::default();
    let mut cursor = CandidateCursor::default();
    let mut compression_scratch = CompressionScratch::default();
    let mut next = || {
        builder
            .build_next_packet(
                &staged.source(CompressionConfig::DISABLED, max_payload),
                &mut cursor,
                &mut compression_scratch,
            )
            .map(Option::unwrap)
    };
    for fragment in 0..2 {
        let packet = next()?;
        assert!(packet.len() <= max_payload);
        assert_eq!(
            packet
                .messages
                .iter()
                .map(|message| message.commit.key)
                .collect::<Vec<_>>(),
            [SendMessageKey::UnreliableFragment(0, fragment)]
        );
    }

    let single_packet = next()?;
    assert!(single_packet.len() <= max_payload);
    assert_eq!(single_packet.messages.len(), 1);
    assert_eq!(
        single_packet.messages[0].commit.channel_id,
        single_channel_id
    );
    Ok(())
}

/// 2000 compressible singles of 64 bytes need more than one packet: the append that no longer
/// compresses within the MTU is rolled back, and the packet seals with the compression kept from
/// the append before it. Every message arrives once, in its packet's order.
#[cfg(feature = "compression_lz4")]
#[test]
fn a_rolled_back_append_seals_with_the_kept_compression() -> Result<(), PacketError> {
    let channel_kind = ChannelKind::of::<Channel1>();
    let channel_id = net_id(channel_kind);
    let mut staged = Staged::default();
    for index in 0..2000 {
        staged.single(
            (channel_kind, channel_id),
            index,
            Bytes::from(vec![(index % 4) as u8; 64]),
        );
    }
    let compression = CompressionConfig {
        min_payload_size: 0,
        ..CompressionConfig::LZ4
    };

    let packets = build_staged_packets(&mut PacketBuilder::default(), &staged, compression)?;
    assert!(packets.len() > 1);
    let mut received = Vec::new();
    for packet in packets {
        assert!(packet.len() <= DEFAULT_MAX_PAYLOAD);
        assert!(packet.compression.is_some());
        let contents = decompress_packet_for_test(packet)?.parse_packet_payload(no_ids)?;
        received.extend(contents[&channel_id].iter().cloned());
    }
    let expected = (0..2000_usize)
        .map(|index| Bytes::from(vec![(index % 4) as u8; 64]))
        .collect::<Vec<_>>();
    assert_eq!(received, expected);
    Ok(())
}

/// Without compression the payload is the pooled buffer itself: the staging buffer, which only a
/// compressed body fills, stays unallocated.
#[test]
fn an_uncompressed_packet_needs_no_staging() -> Result<(), PacketError> {
    let channel_kind = ChannelKind::of::<Channel1>();
    let mut staged = Staged::default();
    for index in 0..64 {
        staged.single(
            (channel_kind, net_id(channel_kind)),
            index,
            Bytes::from(vec![1; 64]),
        );
    }
    let mut builder = PacketBuilder::default();
    let packets = build_staged_packets(&mut builder, &staged, CompressionConfig::DISABLED)?;
    assert!(packets.len() > 1);
    assert_eq!(builder.staging.capacity(), 0);
    Ok(())
}

/// One packet of 128 singles of 64 equal bytes takes two compressions, not one for each single
/// past the limit: the first single past the limit compresses so well that the bound it sets
/// takes the rest, and the seal compresses the whole body once.
#[cfg(feature = "compression_lz4")]
#[test]
fn a_packet_compresses_once_past_the_limit_and_once_at_its_seal() -> Result<(), PacketError> {
    let channel_kind = ChannelKind::of::<Channel1>();
    let channel_id = net_id(channel_kind);
    let mut staged = Staged::default();
    for index in 0..128 {
        staged.single((channel_kind, channel_id), index, Bytes::from(vec![7; 64]));
    }
    let compression = CompressionConfig {
        min_payload_size: 0,
        ..CompressionConfig::LZ4
    };
    let mut builder = PacketBuilder::default();
    let mut cursor = CandidateCursor::default();
    let mut scratch = CompressionScratch::default();
    let packet = builder
        .build_next_packet(
            &staged.source(compression, DEFAULT_MAX_PAYLOAD),
            &mut cursor,
            &mut scratch,
        )?
        .unwrap();
    assert_eq!(packet.messages.len(), 128);
    assert_eq!(scratch.compressions, 2);
    Ok(())
}

/// A body that its bound let grow past what compresses within the room rolls back to the state
/// its last compression verified. Singles of 64 equal bytes take 66 on the wire, after 2 for
/// their batch, so `k` fit the limit; the next one compresses into the room and sets a bound
/// that random singles of 128 bytes pass the room under. The packet seals with the first
/// `k + 1`, and every single arrives once, in order.
#[cfg(feature = "compression_lz4")]
#[test]
fn a_seal_past_the_room_rolls_back_to_the_verified_body() -> Result<(), PacketError> {
    let channel_kind = ChannelKind::of::<Channel1>();
    let channel_id = net_id(channel_kind);
    let mut staged = Staged::default();
    let mut expected = Vec::new();
    for index in 0..20 {
        let bytes = Bytes::from(vec![7; 64]);
        staged.single((channel_kind, channel_id), index, bytes.clone());
        expected.push(bytes);
    }
    for index in 20..60 {
        let bytes = Bytes::from(random_payload(128, usize::try_from(index).unwrap()));
        staged.single((channel_kind, channel_id), index, bytes.clone());
        expected.push(bytes);
    }
    let compression = CompressionConfig {
        min_payload_size: 0,
        ..CompressionConfig::LZ4
    };

    let packets = build_staged_packets(&mut PacketBuilder::default(), &staged, compression)?;
    let k = (DEFAULT_MAX_PAYLOAD - LONG_TICK_LEN - 2) / 66;
    assert_eq!(packets[0].messages.len(), k + 1);
    assert!(packets[0].compression.is_some());
    let mut received = Vec::new();
    for packet in packets {
        assert!(packet.len() <= DEFAULT_MAX_PAYLOAD);
        let packet = if packet.kind.is_compressed() {
            decompress_packet_for_test(packet)?
        } else {
            packet
        };
        received.extend(
            packet.parse_packet_payload(no_ids)?[&channel_id]
                .iter()
                .cloned(),
        );
    }
    assert_eq!(received, expected);
    Ok(())
}
