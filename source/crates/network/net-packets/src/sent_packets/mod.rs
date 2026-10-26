//! The packets this end sent that need an ACK, and what their acknowledgements and their silence
//! tell: delivery, loss by RFC 9002 §6.1's thresholds, RTT samples, and the probe timeout.

use core::time::Duration;
use std::collections::VecDeque;

use campfire_net_core::Instant;
use campfire_net_payload::{PacketNumber, PayloadFate, PayloadToken};

use crate::new_reno::NewReno;
use crate::received_window::{MAX_ACK_DELAY, WINDOW};
use crate::rtt_estimator::{GRANULARITY, RttEstimator};

/// A packet this many numbers below the largest acknowledged is lost (RFC 9002 §6.1.1,
/// `kPacketThreshold`) (11-net-stack/packet-session.md, Loss is found by the packet threshold of 3,
/// or by 9/8 of the RTT).
const PACKET_THRESHOLD: u64 = 3;

/// Losses that span this many probe timeouts with no acknowledgement among them are persistent
/// congestion (RFC 9002 §7.6.1, `kPersistentCongestionThreshold`).
const PERSISTENT_CONGESTION_THRESHOLD: u32 = 3;

/// The numbers below the largest acknowledged that an ACK's bits name.
const ACK_BITS: u64 = WINDOW;

/// A sent packet that needs an ACK, in 32 bytes: its payload's presence and its settlement share
/// one byte of flags, as an `Option<PayloadToken>` alone would take 16.
#[derive(Debug, Clone, Copy)]
struct SentPacket {
    number: PacketNumber,
    sent_at: Instant,
    /// The frame's payload it carried, meaningful under [`Self::CARRIES_PAYLOAD`].
    token: PayloadToken,
    bytes: u32,
    flags: u8,
}

impl SentPacket {
    const CARRIES_PAYLOAD: u8 = 1;
    /// Acknowledged or lost, and kept until every older packet is too.
    const SETTLED: u8 = 1 << 1;

    fn new(
        number: PacketNumber,
        sent_at: Instant,
        bytes: usize,
        token: Option<PayloadToken>,
    ) -> Self {
        Self {
            number,
            sent_at,
            token: token.unwrap_or(PayloadToken(0)),
            bytes: u32::try_from(bytes).expect("a datagram is under 4 GiB"),
            flags: if token.is_some() {
                Self::CARRIES_PAYLOAD
            } else {
                0
            },
        }
    }

    /// The frame's payload it carried, if any.
    const fn token(&self) -> Option<PayloadToken> {
        if self.flags & Self::CARRIES_PAYLOAD == 0 {
            return None;
        }
        Some(self.token)
    }

    fn bytes(&self) -> usize {
        usize::try_from(self.bytes).expect("a u32 fits a usize")
    }

    const fn is_settled(&self) -> bool {
        self.flags & Self::SETTLED != 0
    }

    const fn settle(&mut self) {
        self.flags |= Self::SETTLED;
    }
}

/// What an acknowledgement told: the RTT sample of its largest number, less the peer's ACK delay,
/// when that number was newly acknowledged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AckReport {
    pub(crate) rtt_sample: Option<Duration>,
}

#[derive(Debug, Default)]
pub(crate) struct SentPackets {
    /// In number order; a settled packet stays until the oldest is settled.
    packets: VecDeque<SentPacket>,
    largest_acked: Option<PacketNumber>,
    /// When the oldest packet the thresholds do not call lost yet becomes lost by time.
    loss_time: Option<Instant>,
    last_sent: Option<Instant>,
    /// The probe timeouts in a row with no acknowledgement, which double the next.
    probes: u32,
}

impl SentPackets {
    pub(crate) const fn largest_acked(&self) -> Option<PacketNumber> {
        self.largest_acked
    }

    /// Records a packet of `bytes` that needs an ACK, sent at `sent_at`, carrying `token`.
    pub(crate) fn on_sent(
        &mut self,
        number: PacketNumber,
        sent_at: Instant,
        bytes: usize,
        token: Option<PayloadToken>,
        reno: &mut NewReno,
    ) {
        debug_assert!(
            self.packets.back().is_none_or(|last| last.number < number),
            "packets are sent in number order"
        );
        self.packets
            .push_back(SentPacket::new(number, sent_at, bytes, token));
        self.last_sent = Some(sent_at);
        reno.on_sent(bytes);
    }

    /// Takes an acknowledgement of `largest`, the 64 numbers below it that `bits` names, and the
    /// peer's `ack_delay`, at `now`: each newly acknowledged packet's payload is delivered, the
    /// largest gives an RTT sample when newly acknowledged and its delay is known, and the
    /// thresholds find the losses.
    pub(crate) fn on_ack(
        &mut self,
        ack: Ack,
        now: Instant,
        rtt: &mut RttEstimator,
        reno: &mut NewReno,
        fates: &mut Vec<PayloadFate>,
    ) -> AckReport {
        let Ack {
            largest,
            bits,
            delay,
        } = ack;
        let named = |number: PacketNumber| {
            number == largest || {
                let below = largest.0.wrapping_sub(number.0);
                (1..=ACK_BITS).contains(&below) && bits & (1 << (below - 1)) != 0
            }
        };
        let mut largest_sent_at = None;
        let mut newly_acked = false;
        // The packets are in sent order, and the ACK names numbers within its bits of `largest`.
        let lowest = PacketNumber(largest.0.saturating_sub(ACK_BITS));
        let first = self
            .packets
            .partition_point(|packet| packet.number < lowest);
        for packet in self.packets.range_mut(first..) {
            if packet.number > largest {
                break;
            }
            if packet.is_settled() || !named(packet.number) {
                continue;
            }
            packet.settle();
            newly_acked = true;
            if packet.number == largest {
                largest_sent_at = Some(packet.sent_at);
            }
            if let Some(token) = packet.token() {
                fates.push(PayloadFate::Delivered(token));
            }
            reno.on_acked(packet.bytes(), packet.sent_at);
        }
        if self.largest_acked.is_none_or(|known| largest > known) {
            self.largest_acked = Some(largest);
        }
        if newly_acked {
            self.probes = 0;
        }
        // A delay past what the header holds would make the sample the delay's own length.
        let rtt_sample = largest_sent_at
            .zip(delay)
            .map(|(sent_at, delay)| rtt.update(now.saturating_duration_since(sent_at), delay, now));
        self.detect_losses(now, rtt, reno, fates);
        AckReport { rtt_sample }
    }

    /// Calls each packet below the largest acknowledged lost that the packet threshold or the time
    /// threshold, 9/8 of the larger of the smoothed and the latest RTT, says is, and sets the loss
    /// time for the oldest of the rest (RFC 9002 §6.1).
    pub(crate) fn detect_losses(
        &mut self,
        now: Instant,
        rtt: &RttEstimator,
        reno: &mut NewReno,
        fates: &mut Vec<PayloadFate>,
    ) {
        self.loss_time = None;
        let Some(largest_acked) = self.largest_acked else {
            return;
        };
        let delay = (rtt.max_recent() * 9 / 8).max(GRANULARITY);
        let mut lost_span: Option<LostSpan> = None;
        let mut gap = false;
        for packet in &mut self.packets {
            if packet.number >= largest_acked {
                break;
            }
            if packet.is_settled() {
                gap |= lost_span.is_some();
                continue;
            }
            let by_time = packet.sent_at + delay <= now;
            let by_number = largest_acked.0 - packet.number.0 >= PACKET_THRESHOLD;
            if !(by_time || by_number) {
                let lost_at = packet.sent_at + delay;
                self.loss_time = Some(self.loss_time.map_or(lost_at, |time| time.min(lost_at)));
                gap |= lost_span.is_some();
                continue;
            }
            packet.settle();
            if let Some(token) = packet.token() {
                fates.push(PayloadFate::Lost(token));
            }
            reno.on_lost(packet.bytes(), packet.sent_at, now);
            if rtt
                .first_sample()
                .is_none_or(|first| packet.sent_at <= first)
            {
                continue;
            }
            lost_span = Some(match lost_span {
                None => LostSpan {
                    first: packet.sent_at,
                    last: packet.sent_at,
                    contiguous: true,
                },
                Some(span) => LostSpan {
                    last: packet.sent_at,
                    contiguous: span.contiguous && !gap,
                    ..span
                },
            });
        }
        if let Some(span) = lost_span
            && span.contiguous
            && span.last.saturating_duration_since(span.first) > Self::persistent_congestion(rtt)
        {
            reno.on_persistent_congestion();
        }
        while self.packets.front().is_some_and(SentPacket::is_settled) {
            self.packets.pop_front();
        }
    }

    /// The span of losses with no acknowledgement among them that is persistent congestion:
    /// `(smoothed + max(4·variance, GRANULARITY) + MAX_ACK_DELAY) · 3` (RFC 9002 §7.6.1).
    fn persistent_congestion(rtt: &RttEstimator) -> Duration {
        (rtt.smoothed() + (rtt.variance() * 4).max(GRANULARITY) + MAX_ACK_DELAY)
            * PERSISTENT_CONGESTION_THRESHOLD
    }

    /// When a packet that the thresholds do not call lost yet becomes lost by time.
    pub(crate) const fn loss_time(&self) -> Option<Instant> {
        self.loss_time
    }

    /// When the probe timeout fires: the probe timeout after the last packet that needs an ACK,
    /// doubled for each probe in a row; none while no such packet waits for one.
    pub(crate) fn probe_deadline(&self, rtt: &RttEstimator) -> Option<Instant> {
        if self.packets.iter().all(SentPacket::is_settled) {
            return None;
        }
        let last_sent = self.last_sent?;
        Some(last_sent + rtt.probe_timeout() * 2_u32.saturating_pow(self.probes))
    }

    /// The probe timeout fired, and the packet session sends a probe.
    pub(crate) const fn on_probe(&mut self) {
        self.probes = self.probes.saturating_add(1);
    }
}

/// An acknowledgement, its numbers whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ack {
    pub(crate) largest: PacketNumber,
    pub(crate) bits: u64,
    /// The peer's ACK delay; none when it passed what the header holds.
    pub(crate) delay: Option<Duration>,
}

/// The send times of the first and last packets one detection called lost, and whether no
/// packet sent between them was acknowledged or still waits.
#[derive(Debug, Clone, Copy)]
struct LostSpan {
    first: Instant,
    last: Instant,
    contiguous: bool,
}

#[cfg(test)]
mod tests;
