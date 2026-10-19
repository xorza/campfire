//! One link's packet stream, with no IO: it numbers, seals and acknowledges packets, finds their
//! losses, and keeps the timers, while its endpoint moves its datagrams and the frame's payloads.

use bytes::{Buf, BytesMut};
use core::time::Duration;
use std::collections::VecDeque;

use campfire_net_bytes::ByteArena;
use campfire_net_core::Instant;
use campfire_net_payload::{
    AppCode, OpenedPayload, OutgoingPayload, PacketNumber, PacketSessionQueues, PayloadFate,
    PayloadKind, ReceivePayload, UnlinkReason,
};

use crate::bandwidth_limiter::BandwidthLimiter;
use crate::connection_id::ConnectionId;
use crate::header::{AckFields, Header, PacketType};
use crate::new_reno::NewReno;
use crate::packet_numbers;
use crate::packet_session_setup::PacketSessionSetup;
use crate::protection::Protection;
use crate::received_window::ReceivedWindow;
use crate::rtt_estimator::RttEstimator;
use crate::seal::TAG_LEN;
use crate::sent_packets::{Ack, SentPackets};
use crate::transmit::{Path, Transmit};

/// The bytes of a path packet's challenge.
pub(crate) const CHALLENGE_LEN: usize = 8;

/// The bytes a packet session splits its control packets from, about 20 of the largest: a pass
/// sends a few, the IO drops each once it sent it, and so the arena is whole again by the next
/// pass.
const CONTROL_ARENA_BYTES: usize = 1024;

/// The tries of a path challenge before its candidate is dropped.
const CHALLENGE_TRIES: u8 = 3;

/// A closed packet session answers the peer for this many probe timeouts (RFC 9000 §10.2).
const CLOSING_PROBE_TIMEOUTS: u32 = 3;

/// Failed decryptions past which a sealed packet session ends, as ChaCha20-Poly1305 bears no more
/// for one key (RFC 9001 §6.6).
const INTEGRITY_LIMIT: u64 = 1 << 36;

const _: () = assert!(
    OutgoingPayload::HEADROOM == ConnectionId::LEN + Header::MAX_LEN
        && OutgoingPayload::TAILROOM == TAG_LEN,
    "a payload's headroom holds a connection ID and the longest header, its tailroom the tag"
);

#[derive(Debug)]
pub(crate) struct PacketSession {
    protection: Protection,
    /// The connection ID each datagram this end sends starts with, which the peer's IO routes it
    /// by.
    prefix: Option<ConnectionId>,
    setup: PacketSessionSetup,
    next_number: PacketNumber,
    received: ReceivedWindow,
    sent: SentPackets,
    rtt: RttEstimator,
    reno: NewReno,
    /// The app's rate below the window, if it set one.
    limiter: Option<BandwidthLimiter>,
    last_sent: Instant,
    last_received: Instant,
    /// Payloads opened and not yet handed to the frame.
    inbox: VecDeque<Inbound>,
    /// Payloads handed to the frame, which accepts or refuses each by its next send.
    handed: Vec<Handed>,
    fates: Vec<PayloadFate>,
    rtt_samples: Vec<Duration>,
    failed_opens: u64,
    challenge: Option<Challenge>,
    path_outcome: Option<PathOutcome>,
    state: State,
    /// What the control packets' buffers are split from, so that a Ping, an ACK or a close
    /// allocates nothing once the IO dropped the ones before it.
    control: ByteArena,
    #[cfg(feature = "internals")]
    counts: internals::PacketSessionCounts,
}

#[derive(Debug)]
struct Inbound {
    payload: OpenedPayload,
    received_at: Instant,
}

#[derive(Debug, Clone, Copy)]
struct Handed {
    number: PacketNumber,
    received_at: Instant,
}

#[derive(Debug, Clone, Copy)]
struct Challenge {
    data: [u8; CHALLENGE_LEN],
    sent_at: Instant,
    tries: u8,
}

/// What a path check came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathOutcome {
    /// The candidate answered: it is the peer's address now.
    Validated,
    /// The candidate did not answer its challenges: the peer keeps its address.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Open,
    /// This end closed with `code`, and answers the peer with its close until `until`, at the
    /// 1st, 2nd, 4th and each next power of two of the packets it `received` since: RFC 9000
    /// §10.2.1 limits the replies, which no packet authenticates, to an increasing count.
    Closing {
        code: AppCode,
        until: Instant,
        received: u32,
    },
    Ended(UnlinkReason),
}

/// What became of a received datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Received {
    /// The packet session read it; `largest` when its number is above every number opened before,
    /// as a peer's packet from a new address is, once the peer moved.
    Read { largest: bool },
    /// The packet session dropped it: a replay, a malformed header, or a packet session that ended.
    Dropped,
    /// It did not authenticate.
    Unauthentic,
}

impl PacketSession {
    pub(crate) fn new(
        protection: Protection,
        prefix: Option<ConnectionId>,
        setup: PacketSessionSetup,
        now: Instant,
    ) -> Self {
        debug_assert!(
            setup.keep_alive < setup.idle_timeout,
            "an endpoint checks its packet sessions' setup"
        );
        Self {
            protection,
            prefix,
            setup,
            next_number: PacketNumber(0),
            received: ReceivedWindow::default(),
            sent: SentPackets::default(),
            rtt: RttEstimator::default(),
            reno: NewReno::default(),
            limiter: setup
                .bandwidth
                .map(|limit| BandwidthLimiter::new(limit, now)),
            last_sent: now,
            last_received: now,
            inbox: VecDeque::new(),
            handed: Vec::new(),
            fates: Vec::new(),
            rtt_samples: Vec::new(),
            failed_opens: 0,
            challenge: None,
            path_outcome: None,
            state: State::Open,
            control: ByteArena::new(CONTROL_ARENA_BYTES, 0),
            #[cfg(feature = "internals")]
            counts: internals::PacketSessionCounts::default(),
        }
    }

    /// How the packet session ended, once it did.
    pub(crate) const fn ended(&self) -> Option<UnlinkReason> {
        match self.state {
            State::Ended(reason) => Some(reason),
            State::Open | State::Closing { .. } => None,
        }
    }

    /// The bytes each packet adds to its payload on the wire: the IO's prefix, the longest header
    /// and the tag.
    pub(crate) fn overhead(&self) -> usize {
        self.prefix.map_or(0, |_| ConnectionId::LEN) + Header::MAX_LEN + self.protection.tag_len()
    }

    /// The bytes on the wire the packet session sends at `now`: the window's room within the app's
    /// rate, and none past the packet numbers the peer reads whole, or once the packet session
    /// closed.
    pub(crate) fn room(&self, now: Instant) -> usize {
        let open = matches!(self.state, State::Open);
        if !open || !packet_numbers::is_in_reach(self.next_number, self.sent.largest_acked()) {
            return 0;
        }
        let rate = self
            .limiter
            .as_ref()
            .map_or(usize::MAX, |limiter| limiter.available(now));
        self.reno.room().min(rate)
    }

    /// Hands the frame what it reads at `now`: the opened payloads, which it accepts or refuses by
    /// its next send, the fates, the RTT samples, and the room and the largest payload of a
    /// datagram of `mtu` bytes.
    pub(crate) fn hand_over(&mut self, queues: &mut PacketSessionQueues, mtu: usize, now: Instant) {
        for Inbound {
            payload,
            received_at,
        } in self.inbox.drain(..)
        {
            self.handed.push(Handed {
                number: payload.number,
                received_at,
            });
            queues.push_opened(payload);
        }
        for fate in self.fates.drain(..) {
            queues.push_fate(fate);
        }
        queues.set_rtt_samples(self.rtt_samples.drain(..));
        queues.set_room(self.room(now), self.overhead());
        queues.set_max_payload(mtu.saturating_sub(self.overhead()));
    }

    /// Takes what the frame gives at `now`: the packets it accepted of those handed to it, which
    /// refuses the rest, and its payloads, which go out at once.
    pub(crate) fn take_over(
        &mut self,
        queues: &mut PacketSessionQueues,
        now: Instant,
        out: &mut Vec<Transmit>,
    ) {
        // The frame accepts in the order it was handed the payloads, so one cursor finds each.
        let mut cursor = 0;
        for number in queues.take_accepted() {
            let Some(offset) = self.handed[cursor..]
                .iter()
                .position(|handed| handed.number == number)
            else {
                continue;
            };
            let handed = self.handed[cursor + offset];
            cursor += offset + 1;
            // A later Ping or ACK can take the window past a payload while the frame holds it.
            if self.received.is_new(number) {
                self.received.accept(number, handed.received_at, true);
            }
        }
        for handed in self.handed.drain(..) {
            self.received.settle(handed.number);
        }
        for payload in queues.take_sends() {
            self.send_payload(now, payload, out);
        }
    }

    fn send_payload(&mut self, now: Instant, payload: OutgoingPayload, out: &mut Vec<Transmit>) {
        if !matches!(self.state, State::Open) {
            return;
        }
        let number = self.take_number();
        let acks = self.received.ack_fields(now);
        let header = Header {
            packet_type: PacketType::Payload,
            kind: payload.kind,
            number: packet_numbers::truncate(number),
            acks,
        };
        let datagram = self.seal(number, header, payload.payload);
        if acks.is_some() {
            self.received.acks_sent();
        }
        if let Some(limiter) = &mut self.limiter {
            limiter.consume(datagram.len(), now);
        }
        self.sent.on_sent(
            number,
            now,
            datagram.len(),
            Some(payload.token),
            &mut self.reno,
        );
        self.last_sent = now;
        out.push(Transmit {
            path: Path::Peer,
            datagram,
        });
    }

    /// Sends a packet of the packet session's own of `packet_type`, carrying `content`, on `path`.
    fn send_control(
        &mut self,
        now: Instant,
        packet_type: PacketType,
        content: &[u8],
        path: Path,
        out: &mut Vec<Transmit>,
    ) {
        let number = self.take_number();
        let acks = if packet_type == PacketType::AckOnly {
            self.received.ack_fields(now)
        } else {
            None
        };
        let header = Header {
            packet_type,
            kind: PayloadKind::new(0),
            number: packet_numbers::truncate(number),
            acks,
        };
        let buffer = self.control_buffer(content);
        let datagram = self.seal(number, header, buffer);
        if acks.is_some() {
            self.received.acks_sent();
        }
        if packet_type.is_ack_eliciting() {
            self.sent
                .on_sent(number, now, datagram.len(), None, &mut self.reno);
        }
        self.last_sent = now;
        #[cfg(feature = "internals")]
        self.counts.count(packet_type);
        out.push(Transmit { path, datagram });
    }

    const fn take_number(&mut self) -> PacketNumber {
        let number = self.next_number;
        self.next_number = PacketNumber(number.0 + 1);
        number
    }

    /// A buffer of the control arena holding `content` behind the headroom, with room for the
    /// tag: the arena reuses its chunk once the IO dropped the packets before, and replaces it
    /// while one lives.
    fn control_buffer(&mut self, content: &[u8]) -> BytesMut {
        let len = OutgoingPayload::HEADROOM + content.len() + OutgoingPayload::TAILROOM;
        debug_assert!(
            len <= CONTROL_ARENA_BYTES,
            "an arena holds a control packet"
        );
        let mut buffer = self.control.take(len);
        buffer.resize(OutgoingPayload::HEADROOM, 0);
        buffer.extend_from_slice(content);
        buffer
    }

    /// Writes `header` and the prefix into `buffer`'s headroom, seals its data in place under
    /// `number`, and returns the datagram, which starts at its prefix.
    fn seal(&self, number: PacketNumber, header: Header, mut buffer: BytesMut) -> BytesMut {
        const HEADROOM: usize = OutgoingPayload::HEADROOM;
        let header_len = header.len();
        let prefix_len = self.prefix.map_or(0, |_| ConnectionId::LEN);
        let start = HEADROOM - header_len - prefix_len;
        header.write(&mut buffer[HEADROOM - header_len..HEADROOM]);
        if let Some(prefix) = self.prefix {
            buffer[start..start + prefix_len].copy_from_slice(&prefix.to_bytes());
        }
        if let Protection::Sealed(seal) = &self.protection {
            let (head, data) = buffer.split_at_mut(HEADROOM);
            let tag = seal.seal(number, &head[HEADROOM - header_len..], data);
            buffer.extend_from_slice(&tag);
        }
        buffer.advance(start);
        buffer
    }

    /// Reads `datagram`, its IO's prefix taken off, which arrived at `now` on `path`. A close's
    /// answer to it goes to `out`.
    pub(crate) fn receive(
        &mut self,
        now: Instant,
        mut datagram: BytesMut,
        path: Path,
        out: &mut Vec<Transmit>,
    ) -> Received {
        match self.state {
            State::Ended(_) => return Received::Dropped,
            State::Closing {
                code,
                ref mut received,
                ..
            } => {
                // The peer's own close ends the closing at once, as it hears no more (RFC 9000
                // §10.2.2); any other packet may hear the close again.
                if Header::read(&datagram)
                    .is_ok_and(|header| header.packet_type == PacketType::Close)
                {
                    self.state = State::Ended(UnlinkReason::Closed(code));
                    return Received::Dropped;
                }
                *received = received.saturating_add(1);
                if received.is_power_of_two() {
                    self.send_control(now, PacketType::Close, &code.get().to_be_bytes(), path, out);
                }
                return Received::Dropped;
            }
            State::Open => {}
        }
        let Ok(header) = Header::read(&datagram) else {
            return Received::Dropped;
        };
        let number = self.received.expand(header.number);
        let payload = header.packet_type == PacketType::Payload;
        if self.received.is_pending(number)
            || (payload && self.received.is_below_pending(number))
            || !self.received.is_new(number)
        {
            return Received::Dropped;
        }
        let header_len = header.len();
        let content_len = match &self.protection {
            Protection::Plain => datagram.len() - header_len,
            Protection::Sealed(seal) => {
                let (head, sealed) = datagram.split_at_mut(header_len);
                let Ok(len) = seal.open(number, head, sealed) else {
                    self.failed_opens += 1;
                    if self.failed_opens >= INTEGRITY_LIMIT {
                        self.state = State::Ended(UnlinkReason::ProtocolViolation);
                    }
                    return Received::Unauthentic;
                };
                len
            }
        };
        let largest = self.received.is_largest(number);
        self.received.opened(number);
        self.last_received = now;
        if let Some(acks) = header.acks {
            self.read_acks(now, acks);
        }
        datagram.advance(header_len);
        datagram.truncate(content_len);
        if largest && let Some(floor) = self.received.pending_floor() {
            // The payloads below the pending bits leave the wait, unacknowledged, for their
            // sender to resend.
            self.inbox.retain(|inbound| inbound.payload.number >= floor);
        }
        match header.packet_type {
            PacketType::Payload => {
                self.received.hold(number);
                self.inbox.push_back(Inbound {
                    payload: OpenedPayload {
                        number,
                        kind: header.kind,
                        payload: ReceivePayload::Mutable(datagram),
                    },
                    received_at: now,
                });
            }
            PacketType::AckOnly => self.received.accept(number, now, false),
            PacketType::Ping => self.received.accept(number, now, true),
            PacketType::PathChallenge => {
                self.received.accept(number, now, true);
                if datagram.len() == CHALLENGE_LEN {
                    self.send_control(now, PacketType::PathResponse, &datagram, path, out);
                }
            }
            PacketType::PathResponse => {
                self.received.accept(number, now, true);
                if self
                    .challenge
                    .is_some_and(|challenge| challenge.data[..] == datagram[..])
                {
                    self.challenge = None;
                    self.path_outcome = Some(PathOutcome::Validated);
                }
            }
            PacketType::Close => {
                let code = <[u8; 8]>::try_from(&*datagram).map_or(AppCode::ENDED, |code| {
                    AppCode::from_wire(u64::from_be_bytes(code))
                });
                self.state = State::Ended(UnlinkReason::ClosedByPeer(code));
            }
        }
        Received::Read { largest }
    }

    /// Takes the peer's acknowledgements, read against this end's own largest number sent; ones
    /// that name a number this end never sent are dropped.
    fn read_acks(&mut self, now: Instant, acks: AckFields) {
        let Some(largest_sent) = self.next_number.0.checked_sub(1) else {
            return;
        };
        let largest = packet_numbers::expand(Some(PacketNumber(largest_sent)), acks.largest);
        if largest.0 > largest_sent {
            return;
        }
        let ack = Ack {
            largest,
            bits: acks.bits,
            delay: acks.delay(),
        };
        let report = self
            .sent
            .on_ack(ack, now, &mut self.rtt, &mut self.reno, &mut self.fates);
        if let Some(sample) = report.rtt_sample {
            self.rtt_samples.push(sample);
        }
    }

    /// Runs the timers due at `now`: the close's end, the idle timeout, the loss time, the probe,
    /// the path challenge, the ACK delay and the keep-alive.
    pub(crate) fn poll(&mut self, now: Instant, out: &mut Vec<Transmit>) {
        match self.state {
            State::Ended(_) => return,
            State::Closing { code, until, .. } => {
                if now >= until {
                    self.state = State::Ended(UnlinkReason::Closed(code));
                }
                return;
            }
            State::Open => {}
        }
        if now >= self.last_received + self.setup.idle_timeout {
            self.state = State::Ended(UnlinkReason::TimedOut);
            return;
        }
        if self.sent.loss_time().is_some_and(|time| time <= now) {
            self.sent
                .detect_losses(now, &self.rtt, &mut self.reno, &mut self.fates);
        }
        if self
            .sent
            .probe_deadline(&self.rtt)
            .is_some_and(|deadline| deadline <= now)
        {
            self.sent.on_probe();
            self.send_control(now, PacketType::Ping, &[], Path::Peer, out);
        }
        if let Some(challenge) = self.challenge
            && now >= challenge.sent_at + self.rtt.probe_timeout()
        {
            if challenge.tries >= CHALLENGE_TRIES {
                self.challenge = None;
                self.path_outcome = Some(PathOutcome::Failed);
            } else {
                self.challenge = Some(Challenge {
                    sent_at: now,
                    tries: challenge.tries + 1,
                    ..challenge
                });
                self.send_control(
                    now,
                    PacketType::PathChallenge,
                    &challenge.data,
                    Path::Candidate,
                    out,
                );
            }
        }
        if self
            .received
            .ack_deadline()
            .is_some_and(|deadline| deadline <= now)
        {
            self.send_control(now, PacketType::AckOnly, &[], Path::Peer, out);
        }
        if now >= self.last_sent + self.setup.keep_alive {
            self.send_control(now, PacketType::Ping, &[], Path::Peer, out);
        }
    }

    /// The earliest moment a timer of `poll` is due; none once the packet session ended.
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        match self.state {
            State::Ended(_) => None,
            State::Closing { until, .. } => Some(until),
            State::Open => [
                Some(self.last_received + self.setup.idle_timeout),
                self.sent.loss_time(),
                self.sent.probe_deadline(&self.rtt),
                self.challenge
                    .map(|challenge| challenge.sent_at + self.rtt.probe_timeout()),
                self.received.ack_deadline(),
                Some(self.last_sent + self.setup.keep_alive),
            ]
            .into_iter()
            .flatten()
            .min(),
        }
    }

    /// Closes the packet session with `code` at `now`: its close goes to the peer now, and again in
    /// answer to each packet of the next 3 probe timeouts.
    pub(crate) fn close(&mut self, now: Instant, code: AppCode, out: &mut Vec<Transmit>) {
        if !matches!(self.state, State::Open) {
            return;
        }
        self.state = State::Closing {
            code,
            until: now + self.rtt.probe_timeout() * CLOSING_PROBE_TIMEOUTS,
            received: 0,
        };
        self.send_control(
            now,
            PacketType::Close,
            &code.get().to_be_bytes(),
            Path::Peer,
            out,
        );
    }

    /// The peer's stateless reset ended the packet session: its endpoint holds it no more.
    pub(crate) const fn reset(&mut self) {
        self.state = State::Ended(UnlinkReason::Reset);
    }

    /// Sends a `Ping` at `now`, as each end's first packet tells its peer that the packet session
    /// is open.
    pub(crate) fn ping(&mut self, now: Instant, out: &mut Vec<Transmit>) {
        self.send_control(now, PacketType::Ping, &[], Path::Peer, out);
    }

    /// Checks a candidate path with the challenge `data`, at `now`.
    pub(crate) fn start_challenge(
        &mut self,
        now: Instant,
        data: [u8; CHALLENGE_LEN],
        out: &mut Vec<Transmit>,
    ) {
        self.challenge = Some(Challenge {
            data,
            sent_at: now,
            tries: 1,
        });
        self.send_control(now, PacketType::PathChallenge, &data, Path::Candidate, out);
    }

    /// What the last path check came to, once.
    pub(crate) const fn take_path_outcome(&mut self) -> Option<PathOutcome> {
        self.path_outcome.take()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::header::PacketType;
    use crate::packet_session::PacketSession;

    /// The packets of its own that a packet session sent, which a bench reads: each accumulates
    /// over the packet session's life.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct PacketSessionCounts {
        /// `Ping`s: the first packet, the keep-alives and the probes.
        pub pings: u64,
        /// Packets that carry only an ACK.
        pub ack_only: u64,
    }

    impl PacketSessionCounts {
        pub(crate) const fn count(&mut self, packet_type: PacketType) {
            match packet_type {
                PacketType::Ping => self.pings += 1,
                PacketType::AckOnly => self.ack_only += 1,
                PacketType::Payload
                | PacketType::PathChallenge
                | PacketType::PathResponse
                | PacketType::Close => {}
            }
        }
    }

    impl PacketSession {
        pub(crate) const fn counts(&self) -> PacketSessionCounts {
            self.counts
        }
    }
}

#[cfg(test)]
mod tests;
