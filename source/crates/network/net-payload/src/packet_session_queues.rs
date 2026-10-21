use core::time::Duration;
use std::collections::{VecDeque, vec_deque};
use std::vec;

use crate::packet_number::PacketNumber;
use crate::payload_fate::PayloadFate;
use crate::payload_kind::PayloadKind;
use crate::payload_token::PayloadToken;
use crate::receive_payload::ReceivePayload;
use bytes::BytesMut;

/// What the frame and a link's packet session hand each other: the frame's payloads and the numbers
/// of the packets it accepted, and the packet session's opened payloads, the fate of each sent
/// payload, its RTT samples, its budget and its largest payload. The frame never sees a packet
/// number before the packet session sent the packet, and the packet session never calls the frame.
#[derive(Debug, Default)]
pub struct PacketSessionQueues {
    sends: VecDeque<OutgoingPayload>,
    accepted: Vec<PacketNumber>,
    opened: VecDeque<OpenedPayload>,
    fates: Vec<PayloadFate>,
    rtt_samples: Vec<Duration>,
    /// The bytes on the wire the packet session takes now.
    room: usize,
    /// The bytes each packet adds on the wire to its payload.
    overhead: usize,
    max_payload: usize,
}

/// A payload the frame gives the packet session to send: its data starts at [`Self::HEADROOM`],
/// where the packet session writes its header and its IO's prefix in place, and its buffer holds
/// [`Self::TAILROOM`] bytes past its end, where the seal appends its tag.
#[derive(Debug)]
pub struct OutgoingPayload {
    pub token: PayloadToken,
    pub kind: PayloadKind,
    pub payload: BytesMut,
}

impl OutgoingPayload {
    /// The bytes before the data: a connection ID of 8 and a header of at most 10.
    pub const HEADROOM: usize = 18;
    /// The bytes past the data: the AEAD tag of 16.
    pub const TAILROOM: usize = 16;

    /// The data the payload carries, after its headroom.
    pub fn data(&self) -> &[u8] {
        &self.payload[Self::HEADROOM..]
    }

    /// The largest payload a datagram of `mtu` bytes carries under the packet session's longest
    /// header and its tag, as a sealed client's packet session sends.
    pub const fn max_payload(mtu: usize) -> usize {
        mtu - Self::HEADROOM - Self::TAILROOM
    }
}

/// A payload the packet session opened, and the packet that carried it.
#[derive(Debug, Clone)]
pub struct OpenedPayload {
    pub number: PacketNumber,
    pub kind: PayloadKind,
    pub payload: ReceivePayload,
}

impl PacketSessionQueues {
    /// Queues `payload` for the packet session to send: its data, after its headroom, and the
    /// packet's overhead come off the budget.
    pub fn send(&mut self, token: PayloadToken, kind: PayloadKind, payload: BytesMut) {
        debug_assert!(
            payload.len() >= OutgoingPayload::HEADROOM,
            "a payload starts after its headroom"
        );
        let wire = payload.len() - OutgoingPayload::HEADROOM + self.overhead;
        self.room = self.room.saturating_sub(wire);
        self.sends.push_back(OutgoingPayload {
            token,
            kind,
            payload,
        });
    }

    /// Accepts the packet `number` carried: the packet session acknowledges it, and only it
    /// (11-net-stack/packet-session.md, An ACK means that the transport accepted the packet).
    pub fn accept(&mut self, number: PacketNumber) {
        self.accepted.push(number);
    }

    /// The payloads the packet session opened since the last drain, in the order they arrived.
    pub fn opened(&mut self) -> vec_deque::Drain<'_, OpenedPayload> {
        self.opened.drain(..)
    }

    /// The fates of sent payloads since the last drain.
    pub fn fates(&mut self) -> vec::Drain<'_, PayloadFate> {
        self.fates.drain(..)
    }

    /// The round-trip samples the packet session took since its last hand-over, each less the
    /// peer's ACK delay; each hand-over replaces them, so a frame reads only its own.
    pub fn rtt_samples(&self) -> &[Duration] {
        &self.rtt_samples
    }

    /// The data one more packet may carry in this frame: the packet session's room less the
    /// packet's overhead.
    pub const fn budget(&self) -> usize {
        self.room.saturating_sub(self.overhead)
    }

    /// The largest payload one packet carries.
    pub const fn max_payload(&self) -> usize {
        self.max_payload
    }

    /// The frame's payloads, for the packet session to send.
    pub fn take_sends(&mut self) -> vec_deque::Drain<'_, OutgoingPayload> {
        self.sends.drain(..)
    }

    /// The numbers of the packets the frame accepted, for the packet session to acknowledge.
    pub fn take_accepted(&mut self) -> vec::Drain<'_, PacketNumber> {
        self.accepted.drain(..)
    }

    pub fn push_opened(&mut self, payload: OpenedPayload) {
        self.opened.push_back(payload);
    }

    pub fn push_fate(&mut self, fate: PayloadFate) {
        self.fates.push(fate);
    }

    /// Replaces the round-trip samples with `samples`.
    pub fn set_rtt_samples(&mut self, samples: impl IntoIterator<Item = Duration>) {
        self.rtt_samples.clear();
        self.rtt_samples.extend(samples);
    }

    /// Sets the bytes on the wire the packet session takes now, and those each packet adds.
    pub const fn set_room(&mut self, room: usize, overhead: usize) {
        self.room = room;
        self.overhead = overhead;
    }

    pub const fn set_max_payload(&mut self, max_payload: usize) {
        self.max_payload = max_payload;
    }

    /// Drops everything both sides queued, as a link that unlinked holds nothing of its last
    /// packet session.
    pub fn clear(&mut self) {
        self.sends.clear();
        self.accepted.clear();
        self.opened.clear();
        self.fates.clear();
        self.rtt_samples.clear();
        self.room = 0;
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bytes::BytesMut;

    use crate::mtu::DEFAULT_MTU;
    use crate::packet_number::PacketNumber;
    use crate::packet_session_queues::{OpenedPayload, OutgoingPayload, PacketSessionQueues};
    use crate::receive_payload::ReceivePayload;

    impl OutgoingPayload {
        /// A payload's buffer of `data` after its headroom, as the frame writes one.
        pub fn buffer(data: &[u8]) -> BytesMut {
            let mut payload = BytesMut::zeroed(Self::HEADROOM);
            payload.extend_from_slice(data);
            payload
        }
    }

    impl PacketSessionQueues {
        /// Queues of a packet session that takes every payload: no budget limit, and the largest
        /// payload of a datagram of [`DEFAULT_MTU`].
        pub fn unbounded() -> Self {
            let mut queues = Self::default();
            queues.set_room(usize::MAX, 0);
            queues.set_max_payload(OutgoingPayload::max_payload(DEFAULT_MTU));
            queues
        }

        /// Hands the frame each payload it queued as one its packet session opened, numbered in
        /// order, as a packet session to itself would; returns how many.
        pub fn loop_back(&mut self) -> usize {
            let sends: Vec<_> = self.sends.drain(..).collect();
            let looped = sends.len();
            for (number, send) in (0..).zip(sends) {
                self.push_opened(OpenedPayload {
                    number: PacketNumber(number),
                    kind: send.kind,
                    payload: ReceivePayload::Mutable(BytesMut::from(send.data())),
                });
            }
            looped
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A send takes its data and the packet's overhead off the room: with a room of 50 and an
    /// overhead of 12, the budget is 38; 4 bytes take 16, leaving 34 and a budget of 22; 30 bytes
    /// take 42, leaving none.
    #[test]
    fn a_send_takes_its_data_and_overhead_from_the_room() {
        let mut queues = PacketSessionQueues::default();
        queues.set_room(50, 12);
        assert_eq!(queues.budget(), 38);
        let kind = PayloadKind::new(1);
        queues.send(PayloadToken(1), kind, OutgoingPayload::buffer(b"abcd"));
        assert_eq!(queues.budget(), 22);
        queues.send(PayloadToken(2), kind, OutgoingPayload::buffer(&[0; 30]));
        assert_eq!(queues.budget(), 0);
        let sends: Vec<_> = queues.take_sends().collect();
        assert_eq!(sends[0].data(), b"abcd");
        assert_eq!(
            sends.iter().map(|send| send.token).collect::<Vec<_>>(),
            [PayloadToken(1), PayloadToken(2)]
        );
    }

    /// A clear empties both directions and the room, and keeps the largest payload, which the
    /// IO's MTU sets.
    #[test]
    fn a_clear_empties_both_directions() {
        let mut queues = PacketSessionQueues::default();
        queues.set_room(100, 10);
        queues.set_max_payload(1000);
        queues.send(
            PayloadToken(1),
            PayloadKind::new(0),
            OutgoingPayload::buffer(b""),
        );
        queues.accept(PacketNumber(3));
        queues.push_opened(OpenedPayload {
            number: PacketNumber(4),
            kind: PayloadKind::new(0),
            payload: ReceivePayload::Frozen(bytes::Bytes::new()),
        });
        queues.push_fate(PayloadFate::Lost(PayloadToken(5)));
        queues.set_rtt_samples([Duration::from_millis(6)]);
        queues.clear();
        assert_eq!(queues.take_sends().count(), 0);
        assert_eq!(queues.take_accepted().count(), 0);
        assert_eq!(queues.opened().count(), 0);
        assert_eq!(queues.fates().count(), 0);
        assert!(queues.rtt_samples().is_empty());
        assert_eq!([queues.budget(), queues.max_payload()], [0, 1000]);
    }
}
