use super::*;
use campfire_net_payload::PayloadToken;

use crate::bandwidth_limit::BandwidthLimit;
use crate::open_error::OpenError;
use crate::received_window::MAX_ACK_DELAY;
use crate::rtt_estimator::INITIAL_RTT;
use crate::seal::Seal;

fn at(millis: u64) -> Instant {
    Instant::from_elapsed(Duration::from_millis(millis))
}

/// A seal for tests: each byte XORed with the packet number's low byte, and a tag of the
/// wrapping sum of the header, the plain bytes and the number, repeated.
#[derive(Debug)]
struct TestSeal;

impl TestSeal {
    fn tag(number: PacketNumber, header: &[u8], plain: &[u8]) -> [u8; TAG_LEN] {
        let sum = header
            .iter()
            .chain(plain)
            .fold(number.0 as u8, |sum, byte| sum.wrapping_add(*byte));
        [sum; TAG_LEN]
    }
}

impl Seal for TestSeal {
    fn seal(&self, number: PacketNumber, header: &[u8], payload: &mut [u8]) -> [u8; TAG_LEN] {
        let tag = Self::tag(number, header, payload);
        for byte in payload.iter_mut() {
            *byte ^= number.0 as u8;
        }
        tag
    }

    fn open(
        &self,
        number: PacketNumber,
        header: &[u8],
        sealed: &mut [u8],
    ) -> Result<usize, OpenError> {
        let len = sealed.len().checked_sub(TAG_LEN).ok_or(OpenError)?;
        let (payload, tag) = sealed.split_at_mut(len);
        for byte in payload.iter_mut() {
            *byte ^= number.0 as u8;
        }
        if tag[..] != Self::tag(number, header, payload) {
            return Err(OpenError);
        }
        Ok(len)
    }
}

/// A payload of `data` with its headroom and tailroom.
fn outgoing(token: u64, data: &[u8]) -> BytesMut {
    let mut payload =
        BytesMut::with_capacity(OutgoingPayload::HEADROOM + data.len() + OutgoingPayload::TAILROOM);
    payload.resize(OutgoingPayload::HEADROOM, 0);
    payload.extend_from_slice(data);
    let _ = token;
    payload
}

/// Two sessions and the frame's queues of each, joined by a wire that delivers at once.
#[derive(Debug)]
struct Pair {
    ends: [End; 2],
}

#[derive(Debug)]
struct End {
    session: PacketSession,
    queues: SessionQueues,
}

impl Pair {
    fn new(protection: impl Fn() -> Protection, config: SessionConfig) -> Self {
        let end = || End {
            session: PacketSession::new(protection(), None, config, at(0)),
            queues: SessionQueues::default(),
        };
        Self {
            ends: [end(), end()],
        }
    }

    fn plain() -> Self {
        Self::new(|| Protection::Plain, SessionConfig::default())
    }

    /// End `from` sends `data` as payload `token` at `now`, and its datagrams reach the other end
    /// at `arrival`, or never.
    fn send(
        &mut self,
        from: usize,
        token: u64,
        data: &[u8],
        now: Instant,
        arrival: Option<Instant>,
    ) {
        let end = &mut self.ends[from];
        end.queues.send(
            PayloadToken(token),
            PayloadKind::new(1),
            outgoing(token, data),
        );
        let mut out = Vec::new();
        end.session.take_over(&mut end.queues, now, &mut out);
        if let Some(arrival) = arrival {
            self.deliver(1 - from, out, arrival);
        }
    }

    fn deliver(&mut self, to: usize, transmits: Vec<Transmit>, now: Instant) {
        let mut answers = Vec::new();
        for transmit in transmits {
            self.ends[to]
                .session
                .receive(now, transmit.datagram, transmit.path, &mut answers);
        }
        if !answers.is_empty() {
            self.deliver(1 - to, answers, now);
        }
    }

    /// End `end`'s frame takes what its session opened, and accepts each payload when `accept`
    /// holds, by the session's next take: the data of each, in order.
    fn take(&mut self, end: usize, accept: bool, now: Instant) -> Vec<Vec<u8>> {
        let end = &mut self.ends[end];
        end.session.hand_over(&mut end.queues, 1_200, now);
        let opened: Vec<_> = end.queues.opened().collect();
        let mut data = Vec::new();
        for payload in opened {
            if accept {
                end.queues.accept(payload.number);
            }
            data.push(payload.payload.freeze().to_vec());
        }
        let mut out = Vec::new();
        end.session.take_over(&mut end.queues, now, &mut out);
        assert!(out.is_empty(), "the frame queued no payload");
        data
    }

    /// Runs end `end`'s timers at `now`, and delivers what they send unless `drop` holds: the
    /// datagrams' packet types.
    fn poll(&mut self, end: usize, now: Instant, drop: bool) -> Vec<PacketType> {
        let mut out = Vec::new();
        self.ends[end].session.poll(now, &mut out);
        let types = out
            .iter()
            .map(|transmit| Header::read(&transmit.datagram).unwrap().packet_type)
            .collect();
        if !drop {
            self.deliver(1 - end, out, now);
        }
        types
    }

    fn fates(&mut self, end: usize) -> Vec<PayloadFate> {
        let end = &mut self.ends[end];
        end.session.hand_over(&mut end.queues, 1_200, at(0));
        end.queues.fates().collect()
    }

    /// The samples of `end`'s last hand-over.
    fn rtt_samples(&self, end: usize) -> &[Duration] {
        self.ends[end].queues.rtt_samples()
    }
}

/// A payload crosses in 5 ms and its frame accepts it: the peer's reply, 5 ms after it arrived and
/// 5 ms on the wire, acknowledges it 15 ms after its send, and the sender learns its delivery and
/// an RTT sample of 15 ms less the 5 ms the acknowledgement waited: 10 ms, the two legs.
#[test]
fn an_accepted_payload_is_delivered_and_gives_a_sample() {
    for sealed in [false, true] {
        let mut pair = if sealed {
            Pair::new(
                || Protection::Sealed(Box::new(TestSeal)),
                SessionConfig::default(),
            )
        } else {
            Pair::plain()
        };
        pair.send(0, 7, b"hello", at(10), Some(at(15)));
        assert_eq!(pair.take(1, true, at(17)), [b"hello".to_vec()]);
        pair.send(1, 8, b"reply", at(20), Some(at(25)));
        assert_eq!(pair.take(0, true, at(30)), [b"reply".to_vec()]);
        assert_eq!(
            pair.rtt_samples(0),
            [Duration::from_millis(10)],
            "sealed: {sealed}"
        );
        assert_eq!(pair.fates(0), [PayloadFate::Delivered(PayloadToken(7))]);
        assert!(
            pair.rtt_samples(0).is_empty(),
            "a hand-over replaces the samples"
        );
    }
}

/// A payload the frame refuses is never acknowledged: three later payloads that it accepts call
/// it lost by the packet threshold.
#[test]
fn a_refused_payload_is_never_acknowledged_and_is_lost() {
    let mut pair = Pair::plain();
    pair.send(0, 0, b"refused", at(0), Some(at(0)));
    pair.take(1, false, at(1));
    for token in 1..=3 {
        pair.send(0, token, b"accepted", at(token), Some(at(token)));
        pair.take(1, true, at(token));
    }
    pair.send(1, 100, b"acks", at(5), Some(at(5)));
    pair.take(0, true, at(5));
    assert_eq!(
        pair.fates(0),
        [
            PayloadFate::Delivered(PayloadToken(1)),
            PayloadFate::Delivered(PayloadToken(2)),
            PayloadFate::Delivered(PayloadToken(3)),
            PayloadFate::Lost(PayloadToken(0)),
        ]
    );
}

/// A datagram that arrives twice is read once, before and after its frame decided on it.
#[test]
fn a_replayed_datagram_is_dropped() {
    let mut pair = Pair::plain();
    let end = &mut pair.ends[0];
    end.queues
        .send(PayloadToken(0), PayloadKind::new(1), outgoing(0, b"once"));
    let mut out = Vec::new();
    end.session.take_over(&mut end.queues, at(0), &mut out);
    let datagram = out.pop().unwrap().datagram;
    let receiver = &mut pair.ends[1].session;
    let mut answers = Vec::new();
    for _ in 0..2 {
        receiver.receive(at(1), datagram.clone(), Path::Peer, &mut answers);
    }
    assert_eq!(pair.take(1, true, at(2)), [b"once".to_vec()]);
    let receiver = &mut pair.ends[1].session;
    assert_eq!(
        receiver.receive(at(3), datagram, Path::Peer, &mut answers),
        Received::Dropped
    );
    assert!(answers.is_empty());
}

/// A frame that stalls through 100 payloads finds the newest 64 waiting, as the pending bits hold
/// 64 numbers: payloads 36 to 99. The 36 older ones leave the wait unacknowledged.
#[test]
fn a_stalled_frame_finds_the_newest_payloads_waiting() {
    let mut pair = Pair::plain();
    for token in 0..100_u64 {
        pair.send(0, token, &token.to_be_bytes(), at(0), Some(at(1)));
    }
    let expected: Vec<Vec<u8>> = (36..100_u64)
        .map(|token| token.to_be_bytes().to_vec())
        .collect();
    assert_eq!(pair.take(1, true, at(2)), expected);
}

/// A payload the frame holds while 40 Pings pass it leaves the accept window: the frame's accept
/// of it changes nothing, and a copy of it is dropped as too old.
#[test]
fn an_accept_behind_the_window_is_skipped() {
    let mut pair = Pair::plain();
    let end = &mut pair.ends[0];
    end.queues
        .send(PayloadToken(0), PayloadKind::new(1), outgoing(0, b"held"));
    let mut out = Vec::new();
    end.session.take_over(&mut end.queues, at(0), &mut out);
    let held = out.pop().unwrap().datagram;
    let mut answers = Vec::new();
    pair.ends[1]
        .session
        .receive(at(1), held.clone(), Path::Peer, &mut answers);
    let end = &mut pair.ends[1];
    end.session.hand_over(&mut end.queues, 1_200, at(1));
    for _ in 0..40 {
        let mut pings = Vec::new();
        pair.ends[0].session.ping(at(1), &mut pings);
        pair.deliver(1, pings, at(1));
    }
    let end = &mut pair.ends[1];
    let number = end.queues.opened().next().unwrap().number;
    end.queues.accept(number);
    end.session.take_over(&mut end.queues, at(2), &mut out);
    assert_eq!(
        pair.ends[1]
            .session
            .receive(at(3), held, Path::Peer, &mut answers),
        Received::Dropped
    );
}

/// The control packets split from the session's arena: with each one dropped once it is sent, as
/// an IO does, the arena holds `CONTROL_ARENA_BYTES / (HEADROOM + TAILROOM)` pings, and the next
/// reuses the first one's bytes instead of an allocation.
#[test]
fn control_packets_reuse_their_arena() {
    let mut pair = Pair::plain();
    let fit = CONTROL_ARENA_BYTES / (OutgoingPayload::HEADROOM + OutgoingPayload::TAILROOM);
    let starts: Vec<*const u8> = (0..=fit)
        .map(|_| {
            let mut out = Vec::new();
            pair.ends[0].session.ping(at(0), &mut out);
            out.pop().unwrap().datagram.as_ptr()
        })
        .collect();
    assert_eq!(starts[fit], starts[0]);
    assert!(starts[1..fit].iter().all(|&start| start != starts[0]));
}

/// An end with nothing to send acknowledges an accepted payload 25 ms after it arrived, in an
/// ACK-only packet, and not before.
#[test]
fn an_ack_only_packet_waits_for_the_ack_delay() {
    let mut pair = Pair::plain();
    pair.send(0, 0, b"data", at(0), Some(at(0)));
    pair.take(1, true, at(0));
    let due = at(0) + MAX_ACK_DELAY;
    assert_eq!(pair.ends[1].session.next_deadline(), Some(due));
    assert!(pair.poll(1, at(24), false).is_empty());
    assert_eq!(pair.poll(1, due, false), [PacketType::AckOnly]);
    assert_eq!(pair.fates(0), [PayloadFate::Delivered(PayloadToken(0))]);
}

/// A silent session sends a keep-alive after 1 s, and one that hears nothing for 5 s ends with
/// `TimedOut`.
#[test]
fn a_silent_session_keeps_alive_and_a_deaf_one_times_out() {
    let mut pair = Pair::plain();
    assert_eq!(pair.poll(0, at(999), true), []);
    assert_eq!(pair.poll(0, at(1_000), true), [PacketType::Ping]);
    assert_eq!(pair.ends[0].session.ended(), None);
    pair.poll(0, at(5_000), true);
    assert_eq!(pair.ends[0].session.ended(), Some(UnlinkReason::TimedOut));
}

/// A payload whose acknowledgement does not come is probed at the probe timeout: with no sample,
/// 333 + 4 · 166.5 + 25 = 1,024 ms after its send. The probe needs an ACK too, so the next one
/// counts from it, doubled: 2,048 ms later. A keep-alive of 4 s stays out of the way.
#[test]
fn an_unacknowledged_payload_is_probed_at_the_probe_timeout() {
    let config = SessionConfig {
        keep_alive: Duration::from_secs(4),
        ..SessionConfig::default()
    };
    let mut pair = Pair::new(|| Protection::Plain, config);
    pair.send(0, 0, b"data", at(0), None);
    let probe = at(0) + INITIAL_RTT + INITIAL_RTT * 2 + MAX_ACK_DELAY;
    assert_eq!(probe, at(1_024));
    assert_eq!(pair.ends[0].session.next_deadline(), Some(probe));
    assert_eq!(pair.poll(0, probe, true), [PacketType::Ping]);
    assert_eq!(
        pair.ends[0].session.next_deadline(),
        Some(probe + Duration::from_millis(2_048))
    );
}

/// A close reaches the peer with its code; the closer answers late packets with its close again,
/// at the 1st, 2nd, 4th and 8th of 8 of them, and ends 3 probe timeouts after it closed.
#[test]
fn a_close_carries_its_code_and_answers_for_three_probe_timeouts() {
    let mut pair = Pair::plain();
    let code = AppCode::new(42);
    let mut out = Vec::new();
    pair.ends[0].session.close(at(0), code, &mut out);
    let close = out.pop().unwrap().datagram;
    pair.ends[1]
        .session
        .receive(at(1), close, Path::Peer, &mut out);
    assert_eq!(
        pair.ends[1].session.ended(),
        Some(UnlinkReason::ClosedByPeer(code))
    );
    // A packet the peer sent before it read the close.
    let mut late = Pair::plain();
    late.ends[1]
        .queues
        .send(PayloadToken(0), PayloadKind::new(1), outgoing(0, b"late"));
    late.ends[1]
        .session
        .take_over(&mut late.ends[1].queues, at(1), &mut out);
    let late = out.pop().unwrap().datagram;
    let answered: Vec<usize> = (1..=8)
        .filter(|_| {
            pair.ends[0]
                .session
                .receive(at(2), late.clone(), Path::Peer, &mut out);
            out.pop().is_some_and(|answer| {
                Header::read(&answer.datagram).unwrap().packet_type == PacketType::Close
            })
        })
        .collect();
    assert_eq!(answered, [1, 2, 4, 8]);
    let until = at(0) + Duration::from_millis(1_024) * 3;
    pair.poll(0, until.saturating_sub(Duration::from_nanos(1)), true);
    assert_eq!(pair.ends[0].session.ended(), None);
    pair.poll(0, until, true);
    assert_eq!(
        pair.ends[0].session.ended(),
        Some(UnlinkReason::Closed(code))
    );
}

/// Two ends that close at once each end at the other's close, with their own code, and answer it
/// with nothing.
#[test]
fn two_ends_that_close_at_once_end_at_each_others_close() {
    let mut pair = Pair::plain();
    let codes = [AppCode::new(20), AppCode::new(21)];
    let closes = [0, 1].map(|end| {
        let mut out = Vec::new();
        pair.ends[end].session.close(at(0), codes[end], &mut out);
        out.pop().unwrap().datagram
    });
    for (end, close) in closes.into_iter().enumerate() {
        let mut answers = Vec::new();
        pair.ends[1 - end]
            .session
            .receive(at(1), close, Path::Peer, &mut answers);
        assert!(answers.is_empty());
    }
    for (end, code) in codes.into_iter().enumerate() {
        assert_eq!(
            pair.ends[end].session.ended(),
            Some(UnlinkReason::Closed(code))
        );
    }
}

/// A sealed packet whose bytes changed does not authenticate, and goes back to the IO whole.
#[test]
fn a_tampered_sealed_packet_is_unauthentic() {
    let mut pair = Pair::new(
        || Protection::Sealed(Box::new(TestSeal)),
        SessionConfig::default(),
    );
    let end = &mut pair.ends[0];
    end.queues
        .send(PayloadToken(0), PayloadKind::new(1), outgoing(0, b"sealed"));
    let mut out = Vec::new();
    end.session.take_over(&mut end.queues, at(0), &mut out);
    let mut datagram = out.pop().unwrap().datagram;
    // 3 bytes of header, 6 of data, 16 of tag.
    assert_eq!(datagram.len(), 3 + 6 + TAG_LEN);
    datagram[4] ^= 1;
    let received = pair.ends[1]
        .session
        .receive(at(1), datagram, Path::Peer, &mut out);
    assert_eq!(received, Received::Unauthentic);
}

/// A session with a prefix starts each datagram with it, and counts it in its overhead: 8 + 10
/// with no seal, 8 + 10 + 16 with one.
#[test]
fn a_prefix_starts_each_datagram_and_counts_in_the_overhead() {
    const PREFIX: ConnectionId = ConnectionId::from_random([0x12, 0x34, 0x56, 0x78, 0, 0, 0, 1]);
    let mut session = PacketSession::new(
        Protection::Plain,
        Some(PREFIX),
        SessionConfig::default(),
        at(0),
    );
    assert_eq!(session.overhead(), 18);
    let sealed = PacketSession::new(
        Protection::Sealed(Box::new(TestSeal)),
        Some(PREFIX),
        SessionConfig::default(),
        at(0),
    );
    assert_eq!(sealed.overhead(), 34);
    let mut queues = SessionQueues::default();
    queues.send(PayloadToken(0), PayloadKind::new(3), outgoing(0, b"x"));
    let mut out = Vec::new();
    session.take_over(&mut queues, at(0), &mut out);
    let datagram = &out[0].datagram;
    assert_eq!(datagram[..8], PREFIX.to_bytes());
    let header = Header::read(&datagram[8..]).unwrap();
    assert_eq!(header.kind, PayloadKind::new(3));
    assert_eq!(&datagram[8 + header.len()..], b"x");
}

/// A candidate that echoes its challenge is validated; one that does not answer fails after 3
/// tries, each a probe timeout apart.
#[test]
fn a_path_check_validates_an_answer_and_fails_silence() {
    let mut pair = Pair::plain();
    let mut out = Vec::new();
    pair.ends[0]
        .session
        .start_challenge(at(0), [9; CHALLENGE_LEN], &mut out);
    pair.deliver(1, out, at(1));
    assert_eq!(
        pair.ends[0].session.take_path_outcome(),
        Some(PathOutcome::Validated)
    );

    let mut pair = Pair::plain();
    let mut out = Vec::new();
    pair.ends[0]
        .session
        .start_challenge(at(0), [9; CHALLENGE_LEN], &mut out);
    assert_eq!(out[0].path, Path::Candidate);
    let timeout = Duration::from_millis(1_024);
    let mut now = at(0);
    for _ in 0..2 {
        now += timeout;
        let mut out = Vec::new();
        pair.ends[0].session.poll(now, &mut out);
        assert!(out.iter().any(|transmit| transmit.path == Path::Candidate));
    }
    pair.poll(0, now + timeout, true);
    assert_eq!(
        pair.ends[0].session.take_path_outcome(),
        Some(PathOutcome::Failed)
    );
}

/// The room is the window's, and none past the numbers the peer reads whole.
#[test]
fn the_room_stops_where_the_peer_cannot_read_a_number() {
    let mut session = PacketSession::new(Protection::Plain, None, SessionConfig::default(), at(0));
    assert_eq!(session.room(at(0)), 12_000);
    session.next_number = PacketNumber(32_766);
    assert_eq!(session.room(at(0)), 0);
}

/// An app's rate holds the room below the window: a burst of 2,000 at 1,000 B/s. A payload of
/// 1,000 bytes and its header of 3, which acknowledges nothing yet, take 1,003 of it, and 100 ms
/// refill 100: 2,000 − 1,003 + 100 = 1,097.
#[test]
fn the_apps_rate_holds_the_room_below_the_window() {
    let config = SessionConfig {
        bandwidth: Some(BandwidthLimit::new(1_000).with_burst(2_000)),
        ..SessionConfig::default()
    };
    let mut session = PacketSession::new(Protection::Plain, None, config, at(0));
    assert_eq!(session.room(at(0)), 2_000);
    let mut queues = SessionQueues::default();
    let mut payload = BytesMut::zeroed(OutgoingPayload::HEADROOM);
    payload.extend_from_slice(&[0; 1_000]);
    queues.send(PayloadToken(1), PayloadKind::new(0), payload);
    session.take_over(&mut queues, at(0), &mut Vec::new());
    assert_eq!(session.room(at(100)), 1_097);
}

/// Datagrams of each type a session sends: a payload that acknowledges the peer's, a ping, a
/// path challenge and a close, each under `protection`.
fn datagrams_of_each_type(protection: impl Fn() -> Protection) -> Vec<BytesMut> {
    let mut pair = Pair::new(protection, SessionConfig::default());
    pair.send(0, 1, b"first", at(10), Some(at(15)));
    pair.take(1, true, at(16));
    let end = &mut pair.ends[1];
    end.queues
        .send(PayloadToken(2), PayloadKind::new(2), outgoing(2, b"reply"));
    let mut out = Vec::new();
    end.session.take_over(&mut end.queues, at(20), &mut out);
    end.session.ping(at(21), &mut out);
    end.session
        .start_challenge(at(22), [9; CHALLENGE_LEN], &mut out);
    end.session.close(at(23), AppCode::new(40), &mut out);
    out.into_iter().map(|transmit| transmit.datagram).collect()
}

/// A seeded mutation test of the session's parsers: 2,048 mutations of each datagram type reach
/// a fresh session of each protection, and none panics; a session reads a mutation only when its
/// header parses. The test seal's tag is a byte sum, which a mutation can keep, so the UDP
/// crate's test with ChaCha20-Poly1305 shows that a seal refuses every changed datagram.
#[test]
fn mutated_datagrams_are_refused_or_read() {
    use campfire_net_bytes::internals::Mutations;

    type Case = (&'static str, fn() -> Protection);
    let protections: [Case; 2] = [
        ("plain", || Protection::Plain),
        ("sealed", || Protection::Sealed(Box::new(TestSeal))),
    ];
    let mut mutations = Mutations::new(0x5e55_1011);
    let mut mutated = Vec::new();
    for (name, protection) in protections {
        let [mut read, mut refused] = [0; 2];
        for original in datagrams_of_each_type(protection) {
            let mut receiver =
                PacketSession::new(protection(), None, SessionConfig::default(), at(0));
            for _ in 0..2_048 {
                mutations.mutate(&original, &mut mutated);
                let parses = Header::read(&mutated).is_ok();
                let received = receiver.receive(
                    at(30),
                    BytesMut::from(&*mutated),
                    Path::Peer,
                    &mut Vec::new(),
                );
                if matches!(received, Received::Read { .. }) {
                    assert!(parses, "{name}: a read datagram has a header");
                    read += 1;
                } else {
                    refused += 1;
                }
            }
        }
        assert!(
            read > 0 && refused > 0,
            "{name}: {read} read, {refused} refused"
        );
    }
}
