use core::ops::Range;
use std::collections::VecDeque;

use campfire_net_core::NetId;
use campfire_net_core::NetTick;
use campfire_net_payload::PayloadToken;

use crate::send_message_key::SendMessageKey;

/// The packets a transport gave its packet session and whose fate the packet session has not
/// reported: the tick each carried, which the tick encoder learns at its delivery, and the reliable
/// messages each carried, whose channels learn its fate.
///
/// The transport numbers its packets in order, so a packet's place is its token's distance from
/// the first's; its messages are a range of one buffer in the same order. A fate leaves its entry
/// empty until the entries before it are, as fates come in any order.
#[derive(Debug, Default)]
pub(crate) struct UnsettledPackets {
    /// The token of `packets`' first entry.
    first: u64,
    packets: VecDeque<Option<UnsettledPacket>>,
    /// The reliable messages of the packets, in token order.
    messages: VecDeque<SentMessage>,
    /// The index that `messages`' first entry has counted from the start of the transport.
    first_message: u64,
}

/// A packet whose fate is not known yet.
#[derive(Debug, Clone)]
pub(crate) struct UnsettledPacket {
    pub(crate) tick: NetTick,
    /// Its reliable messages, as indices counted from the start of the transport.
    messages: Range<u64>,
}

/// A reliable message or fragment that a packet carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SentMessage {
    pub(crate) channel_id: NetId,
    pub(crate) key: SendMessageKey,
}

impl UnsettledPackets {
    /// Records the packet of `token`, the next one the transport numbered, at `tick`.
    pub(crate) fn push(&mut self, token: PayloadToken, tick: NetTick) {
        if self.packets.is_empty() {
            self.first = token.0;
        }
        debug_assert_eq!(
            token.0,
            self.first + count(self.packets.len()),
            "the transport numbers its packets in order"
        );
        let end = self.first_message + count(self.messages.len());
        self.packets.push_back(Some(UnsettledPacket {
            tick,
            messages: end..end,
        }));
    }

    /// Records that the last packet pushed carries `message`.
    pub(crate) fn push_message(&mut self, message: SentMessage) {
        let packet = self
            .packets
            .back_mut()
            .and_then(Option::as_mut)
            .expect("a message goes in a packet pushed before it");
        packet.messages.end += 1;
        self.messages.push_back(message);
    }

    /// Takes the packet of `token`, whose fate the packet session reports once; a token of a packet
    /// before a reset names none.
    pub(crate) fn take(&mut self, token: PayloadToken) -> Option<UnsettledPacket> {
        let slot = usize::try_from(token.0.checked_sub(self.first)?).ok()?;
        self.packets.get_mut(slot)?.take()
    }

    /// The reliable messages `packet` carried.
    pub(crate) fn messages(
        &self,
        packet: &UnsettledPacket,
    ) -> impl Iterator<Item = SentMessage> + '_ {
        let start = usize::try_from(packet.messages.start - self.first_message).unwrap();
        let end = usize::try_from(packet.messages.end - self.first_message).unwrap();
        self.messages.range(start..end).copied()
    }

    /// Drops the entries before the first packet whose fate is not known, and their messages.
    pub(crate) fn release(&mut self) {
        while self.packets.front().is_some_and(Option::is_none) {
            self.packets.pop_front();
            self.first += 1;
        }
        let end = self.first_message + count(self.messages.len());
        let kept = self
            .packets
            .iter()
            .flatten()
            .next()
            .map_or(end, |packet| packet.messages.start);
        let dropped = usize::try_from(kept - self.first_message).unwrap();
        self.messages.drain(..dropped);
        self.first_message = kept;
    }

    pub(crate) fn clear(&mut self) {
        self.packets.clear();
        self.messages.clear();
        self.first_message = 0;
    }
}

fn count(len: usize) -> u64 {
    u64::try_from(len).unwrap()
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::unsettled_packets::UnsettledPackets;

    impl UnsettledPackets {
        /// The packets whose reliable messages wait for their fate.
        pub(crate) fn with_messages(&self) -> usize {
            self.packets
                .iter()
                .flatten()
                .filter(|packet| !packet.messages.is_empty())
                .count()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_id::MessageId;

    fn message(id: u16) -> SentMessage {
        SentMessage {
            channel_id: NetId::new(0),
            key: SendMessageKey::of_id(MessageId(id), 0),
        }
    }

    /// Packets 10 to 12 carry messages 0 and 1, none, and 2. The fate of 11 drops nothing, as 10
    /// waits before it; the fate of 10 then drops both entries and messages 0 and 1, and 12 keeps
    /// message 2. A fate taken twice, or of a token before the first, finds nothing.
    #[test]
    fn a_fate_takes_its_packet_and_releases_the_ones_before() {
        let mut sent = UnsettledPackets::default();
        sent.push(PayloadToken(10), NetTick(5));
        sent.push_message(message(0));
        sent.push_message(message(1));
        sent.push(PayloadToken(11), NetTick(5));
        sent.push(PayloadToken(12), NetTick(6));
        sent.push_message(message(2));
        assert_eq!(sent.with_messages(), 2);

        let eleven = sent.take(PayloadToken(11)).unwrap();
        assert_eq!(sent.messages(&eleven).count(), 0);
        sent.release();
        assert_eq!(sent.packets.len(), 3);

        let ten = sent.take(PayloadToken(10)).unwrap();
        assert_eq!(ten.tick, NetTick(5));
        assert_eq!(
            sent.messages(&ten).collect::<Vec<_>>(),
            [message(0), message(1)]
        );
        sent.release();
        assert_eq!((sent.first, sent.packets.len()), (12, 1));
        assert_eq!(sent.messages, [message(2)]);
        assert_eq!(sent.with_messages(), 1);

        assert!(sent.take(PayloadToken(10)).is_none());
        assert!(sent.take(PayloadToken(9)).is_none());
        let twelve = sent.take(PayloadToken(12)).unwrap();
        assert_eq!(sent.messages(&twelve).collect::<Vec<_>>(), [message(2)]);
        assert!(sent.take(PayloadToken(12)).is_none());
        sent.release();
        assert!(sent.packets.is_empty() && sent.messages.is_empty());
        assert_eq!(sent.with_messages(), 0);

        // The next packet starts the ring again at its own token.
        sent.push(PayloadToken(13), NetTick(7));
        assert_eq!(sent.take(PayloadToken(13)).unwrap().tick, NetTick(7));
    }
}
