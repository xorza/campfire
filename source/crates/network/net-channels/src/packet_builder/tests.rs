use bevy_platform::collections::HashMap;
use bytes::Bytes;
use bytes::BytesMut;
use campfire_net_core::TickInstant;
use campfire_net_payload::DEFAULT_MTU;

use super::*;
use crate::channel::Channel;
use crate::channel_type_id::ChannelTypeId;
use crate::error::ReceiveError;
use crate::fragmented_message::FragmentedMessage;
use crate::message_id::MessageId;
use crate::packet::FRAGMENT_LEN;
use crate::queued_message::QueuedMessage;
use crate::send_candidate::{CandidateRank, CandidateSink};
use crate::tick_encoder::TickEncoder;

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
fn net_id(channel: ChannelTypeId) -> NetId {
    let channels = [
        ChannelTypeId::of::<Channel1>(),
        ChannelTypeId::of::<Channel2>(),
        ChannelTypeId::of::<Channel3>(),
    ];
    let index = channels.iter().position(|known| *known == channel).unwrap();
    NetId::new(u16::try_from(index).unwrap())
}

/// Candidates with the messages they name, staged without channels: none is committed, so a
/// fragmented message's fragments all stay due.
#[derive(Debug, Default)]
struct Staged {
    candidates: Vec<SendCandidate>,
    /// Each message by its channel and key, a fragmented one's index offset by 100.
    messages: HashMap<(NetId, u32), QueuedMessage>,
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
    fn single(&mut self, (_, channel_id): (ChannelTypeId, NetId), index: u32, bytes: Bytes) {
        let single = SingleBody::new(None, bytes);
        let rank = CandidateRank {
            priority: 1.0,
            send_order: index,
        };
        self.sink(channel_id).push(
            SendMessageKey::new(index, 0),
            rank,
            MessageRef::Single(&single),
        );
        self.messages
            .insert((channel_id, index), QueuedMessage::whole(single));
    }

    /// Stages one fragmented message of priority 1 at the queue index `index`.
    fn fragmented(
        &mut self,
        (_, channel_id): (ChannelTypeId, NetId),
        index: u32,
        message: FragmentedMessage,
    ) {
        let rank = CandidateRank {
            priority: 1.0,
            send_order: index,
        };
        self.sink(channel_id).push(
            SendMessageKey::new(100 + index, 0),
            rank,
            MessageRef::Fragment(message.fragment(0)),
        );
        self.messages.insert(
            (channel_id, 100 + index),
            QueuedMessage::Fragmented(message),
        );
    }

    /// The source of a flush whose packets carry a long tick, as before the peer read one.
    fn source(&self, max_payload: usize) -> PacketSource<'_, Self> {
        PacketSource {
            tick: TickEncoder::default().encode(TickInstant::ZERO),
            candidates: self.candidates.as_slice(),
            messages: self,
            max_payload,
        }
    }
}

impl CandidateMessages for Staged {
    fn message(&self, channel_id: NetId, key: SendMessageKey) -> MessageRef<'_> {
        self.messages[&(channel_id, key.message)].part(key.part)
    }

    fn next_due(&self, channel_id: NetId, key: SendMessageKey) -> Option<SendMessageKey> {
        self.messages[&(channel_id, key.message)]
            .next_due(key.part + 1)
            .map(|next| key.with_part(next))
    }
}

/// A message of `len` bytes, split at `fragment_len`.
fn split(message_id: MessageId, len: usize, fragment_len: usize) -> FragmentedMessage {
    FragmentedMessage::split(message_id, Bytes::from(vec![9u8; len]), fragment_len)
}

fn build_staged_packets(builder: &mut PacketBuilder, staged: &Staged) -> Vec<Packet> {
    let mut cursor = CandidateCursor::default();
    core::iter::from_fn(|| {
        builder.build_next_packet(&staged.source(DEFAULT_MAX_PAYLOAD), &mut cursor)
    })
    .collect()
}

/// No message of the test channels carries an id.
fn no_ids(_: NetId) -> bool {
    false
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
fn the_payloads_share_chunks_that_come_back_once_they_drop() {
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
    };

    let bytes = packet.take_send_payload();

    assert_eq!(bytes.len(), DEFAULT_MTU);
    assert_eq!(bytes.capacity(), DEFAULT_MTU + OutgoingPayload::TAILROOM);
    assert_eq!(packet.payload.capacity(), 0);
}

#[test]
fn the_builder_packs_singles_of_several_channels_in_one_packet() -> Result<(), ReceiveError> {
    let channels = [
        ChannelTypeId::of::<Channel1>(),
        ChannelTypeId::of::<Channel2>(),
        ChannelTypeId::of::<Channel3>(),
    ]
    .map(|channel| {
        let id = net_id(channel);
        (channel, id)
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

    let mut packets = build_staged_packets(&mut PacketBuilder::default(), &staged);
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
fn the_builder_splits_singles_at_the_mtu() -> Result<(), ReceiveError> {
    let channels = [
        ChannelTypeId::of::<Channel1>(),
        ChannelTypeId::of::<Channel2>(),
        ChannelTypeId::of::<Channel3>(),
    ]
    .map(|channel| {
        let id = net_id(channel);
        (channel, id)
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

    let packets = build_staged_packets(&mut PacketBuilder::default(), &staged);
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
fn a_fragment_packet_keeps_to_the_mtu_under_the_largest_overhead() {
    let channel_type_id = ChannelTypeId::of::<Channel1>();
    let channel_id = 64;
    let message = split(MessageId(1024), FRAGMENT_LEN * 64 + 1, FRAGMENT_LEN);
    let expected_packet_count = usize::from(message.fragments());
    assert_eq!(expected_packet_count, 65);
    let mut staged = Staged::default();
    staged.fragmented((channel_type_id, NetId::new(channel_id)), 0, message);

    let packets = build_staged_packets(&mut PacketBuilder::default(), &staged);
    assert_eq!(packets.len(), expected_packet_count);
    assert!(
        packets
            .iter()
            .all(|packet| packet.len() <= DEFAULT_MAX_PAYLOAD)
    );
}

/// Two messages of two fragments of 900 bytes each go one fragment to a packet, and the
/// packet of each one's last fragment fills with two of the five singles of 100 bytes; the
/// fifth single goes alone.
#[test]
fn staged_builder_fills_each_final_fragment_with_singles() {
    let channel_type_id = ChannelTypeId::of::<Channel1>();
    let channel_id = net_id(channel_type_id);
    let mut staged = Staged::default();

    for index in 0..2 {
        staged.fragmented(
            (channel_type_id, channel_id),
            index,
            split(MessageId(u16::try_from(index).unwrap()), 1800, 900),
        );
    }
    for index in 0..5 {
        staged.single(
            (channel_type_id, channel_id),
            index,
            Bytes::from(vec![index as u8; 100]),
        );
    }

    let packets = build_staged_packets(&mut PacketBuilder::default(), &staged);

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
    let single = |index: u32| SendMessageKey::new(index, 0);
    let fragment = |index: u32, part| SendMessageKey::new(100 + index, part);
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
}

#[test]
fn the_builder_splits_a_channel_batch_at_255_messages() -> Result<(), ReceiveError> {
    let channel_type_id = ChannelTypeId::of::<Channel1>();
    let channel_id = net_id(channel_type_id);
    let mut staged = Staged::default();
    for index in 0..300 {
        staged.single((channel_type_id, channel_id), index, Bytes::new());
    }

    let mut builder = PacketBuilder::default();
    let mut cursor = CandidateCursor::default();
    let packet = builder
        .build_next_packet(&staged.source(DEFAULT_MAX_PAYLOAD), &mut cursor)
        .unwrap();
    assert_eq!(packet.messages.len(), 300);
    let contents = packet.parse_packet_payload(no_ids)?;
    assert_eq!(contents[&channel_id].len(), 300);
    assert!(
        builder
            .build_next_packet(&staged.source(DEFAULT_MAX_PAYLOAD), &mut cursor)
            .is_none()
    );
    Ok(())
}

/// A message's last fragment leaves a tail of 1 byte in a packet of 128, under the 4 that a
/// single takes on channel 64, whose id takes 2: the single does not fit, and goes in a packet of
/// its own. A fragment of 116 bytes fills the 128 under 5 for the long tick, 1 for the channel, 2
/// for the message id, 1 each for the index and the count, and 2 for the length; the last, of
/// 115, takes 127.
#[test]
fn a_tail_after_a_last_fragment_takes_the_width_of_its_single_channel() {
    let max_payload = 128;
    let fragment_len = 115;
    let fragment_channel_id = NetId::new(0);
    let single_channel_id = NetId::new(64);
    assert!(single_channel_id.bytes_len() > fragment_channel_id.bytes_len());

    let single = SingleBody::new(None, Bytes::new());
    let single_wire_len = single_channel_id.bytes_len() + 1 + single.bytes_len();
    assert_eq!(single_wire_len, 4);
    let packet_len = |message: &FragmentedMessage, index| {
        let mut written = Vec::new();
        message.fragment(index).write(&mut written);
        EncodedTick::MAX_LEN + fragment_channel_id.bytes_len() + written.len()
    };
    let message = split(MessageId(0), 2 * fragment_len - 1, fragment_len);
    assert_eq!(packet_len(&message, 0), max_payload);
    assert_eq!(packet_len(&message, 1), max_payload - 1);

    let mut staged = Staged::default();
    staged.fragmented(
        (ChannelTypeId::of::<Channel1>(), fragment_channel_id),
        0,
        message,
    );
    staged.single(
        (ChannelTypeId::of::<Channel2>(), single_channel_id),
        0,
        Bytes::new(),
    );

    let mut builder = PacketBuilder::default();
    let mut cursor = CandidateCursor::default();
    let mut next = || {
        builder
            .build_next_packet(&staged.source(max_payload), &mut cursor)
            .unwrap()
    };
    for fragment in 0..2 {
        let packet = next();
        assert!(packet.len() <= max_payload);
        assert_eq!(
            packet
                .messages
                .iter()
                .map(|message| message.commit.key)
                .collect::<Vec<_>>(),
            [SendMessageKey::new(100, fragment)]
        );
    }

    let single_packet = next();
    assert!(single_packet.len() <= max_payload);
    assert_eq!(single_packet.messages.len(), 1);
    assert_eq!(
        single_packet.messages[0].commit.channel_id,
        single_channel_id
    );
}

/// Singles fill each packet to its payload.
#[test]
fn singles_fill_each_packet_to_its_payload() {
    let channel_type_id = ChannelTypeId::of::<Channel1>();
    let mut staged = Staged::default();
    for index in 0..64 {
        staged.single(
            (channel_type_id, net_id(channel_type_id)),
            index,
            Bytes::from(vec![1; 64]),
        );
    }
    let mut builder = PacketBuilder::default();
    let packets = build_staged_packets(&mut builder, &staged);
    // Each message of 64 bytes takes 66 and a packet's header 8, so 17 fit the payload's 1,166
    // bytes, 8 + 17 · 66 = 1,130, and an 18th would pass it: 64 = 3 · 17 + 13, the last packet
    // 8 + 13 · 66 = 866.
    let lengths: Vec<usize> = packets.iter().map(Packet::len).collect();
    assert_eq!(lengths, [1130, 1130, 1130, 866]);
}
