use super::*;
use crate::new_reno::MINIMUM_WINDOW;

fn at(millis: u64) -> Instant {
    Instant::from_elapsed(Duration::from_millis(millis))
}

/// Packets 0 to `count` − 1, each of 100 bytes and its own token, sent 10 ms apart from 0.
fn sent(count: u64) -> (SentPackets, NewReno) {
    let mut packets = SentPackets::default();
    let mut reno = NewReno::default();
    for number in 0..count {
        packets.on_sent(
            PacketNumber(number),
            at(number * 10),
            100,
            Some(PayloadToken(number)),
            &mut reno,
        );
    }
    (packets, reno)
}

fn ack(largest: u64, bits: u32) -> Ack {
    Ack {
        largest: PacketNumber(largest),
        bits,
        delay: Duration::ZERO,
    }
}

/// An ACK of 4 with the bits of 3 and 2 delivers 4, 3 and 2, and the packet threshold of 3 calls
/// 1 (3 below 4) and 0 (4 below) lost: 500 bytes were in flight, and 0 are after.
#[test]
fn the_packet_threshold_calls_three_below_the_largest_lost() {
    let (mut packets, mut reno) = sent(5);
    let mut rtt = RttEstimator::default();
    let mut fates = Vec::new();
    packets.on_ack(ack(4, 0b11), at(45), &mut rtt, &mut reno, &mut fates);
    assert_eq!(
        fates,
        [
            PayloadFate::Delivered(PayloadToken(2)),
            PayloadFate::Delivered(PayloadToken(3)),
            PayloadFate::Delivered(PayloadToken(4)),
            PayloadFate::Lost(PayloadToken(0)),
            PayloadFate::Lost(PayloadToken(1)),
        ]
    );
    assert_eq!(reno.bytes_in_flight(), 0);
    assert_eq!(packets.probe_deadline(&rtt), None);
}

/// The largest newly acknowledged gives the RTT sample: 4 was sent at 40 ms and acknowledged at
/// 100 ms with 10 ms of ACK delay, 60 − 10 = 50 ms. An ACK that names it again gives none.
#[test]
fn the_largest_newly_acknowledged_gives_the_rtt_sample() {
    let (mut packets, mut reno) = sent(5);
    let mut rtt = RttEstimator::default();
    let mut fates = Vec::new();
    let delayed = Ack {
        delay: Duration::from_millis(10),
        ..ack(4, 0)
    };
    let report = packets.on_ack(delayed, at(100), &mut rtt, &mut reno, &mut fates);
    assert_eq!(report.rtt_sample, Some(Duration::from_millis(50)));
    assert_eq!(rtt.smoothed(), Duration::from_millis(60));
    let report = packets.on_ack(ack(4, 0b1), at(110), &mut rtt, &mut reno, &mut fates);
    assert_eq!(report.rtt_sample, None);
}

/// One below the largest is not lost by number, so it waits for the time threshold: with an RTT
/// of 80 ms, 9/8 · 80 = 90 ms after its send at 0, at 90 ms.
#[test]
fn a_packet_the_threshold_spares_is_lost_by_time() {
    let (mut packets, mut reno) = sent(2);
    let mut rtt = RttEstimator::default();
    rtt.update(Duration::from_millis(80), Duration::ZERO);
    let mut fates = Vec::new();
    packets.on_ack(ack(1, 0), at(50), &mut rtt, &mut reno, &mut fates);
    // The sample of 1, sent at 10 ms, is 40 ms, and the latest; 80 ms stays the larger.
    assert_eq!(fates, [PayloadFate::Delivered(PayloadToken(1))]);
    let loss_time = packets.loss_time().unwrap();
    assert_eq!(loss_time, at(0) + rtt.max_recent() * 9 / 8);
    packets.detect_losses(
        loss_time.saturating_sub(Duration::from_nanos(1)),
        &rtt,
        &mut reno,
        &mut fates,
    );
    assert_eq!(fates.len(), 1);
    packets.detect_losses(loss_time, &rtt, &mut reno, &mut fates);
    assert_eq!(fates[1], PayloadFate::Lost(PayloadToken(0)));
}

/// The probe timeout runs from the last packet that needs an ACK, and doubles with each probe:
/// with no sample, 1,024 ms after the send at 20 ms, then 2,048 ms after it.
#[test]
fn the_probe_timeout_doubles_with_each_probe() {
    let (mut packets, _) = sent(3);
    let rtt = RttEstimator::default();
    assert_eq!(
        packets.probe_deadline(&rtt),
        Some(at(20) + Duration::from_millis(1_024))
    );
    packets.on_probe();
    assert_eq!(
        packets.probe_deadline(&rtt),
        Some(at(20) + Duration::from_millis(2_048))
    );
}

/// Losses with no acknowledgement among them that span more than three probe timeouts collapse
/// the window. Packets 0 to 20 go 10 ms apart; after a sample of 10 ms, an ACK of 20 alone at
/// 205 ms gives a sample of 5 ms: smoothed (7 · 10 + 5) / 8 = 9.375 ms, variance (3 · 5 + 5) / 4
/// = 5 ms. It calls 0 to 17 lost by number, and 18 and 19 by time, 9/8 · 9.375 ms after their
/// sends. They span 190 ms, past (9.375 + 4 · 5 + 25) · 3 = 163.125 ms.
#[test]
fn losses_over_three_probe_timeouts_are_persistent_congestion() {
    let (mut packets, mut reno) = sent(21);
    let mut rtt = RttEstimator::default();
    rtt.update(Duration::from_millis(10), Duration::ZERO);
    let mut fates = Vec::new();
    packets.on_ack(ack(20, 0), at(205), &mut rtt, &mut reno, &mut fates);
    let lost = fates
        .iter()
        .filter(|fate| matches!(fate, PayloadFate::Lost(_)))
        .count();
    assert_eq!(lost, 20);
    assert_eq!(reno.window(), MINIMUM_WINDOW);

    // An acknowledgement among the losses breaks the span: the window only halves.
    let (mut packets, mut reno) = sent(21);
    let mut fates = Vec::new();
    packets.on_ack(ack(20, 1 << 9), at(205), &mut rtt, &mut reno, &mut fates);
    assert_eq!(reno.window(), 6_000);
}

/// A packet that carried no payload reports no fate, one that carried token 0 reports it, and
/// each record takes 32 bytes.
#[test]
fn a_packet_reports_the_payload_it_carried_alone() {
    let mut packets = SentPackets::default();
    let mut reno = NewReno::default();
    packets.on_sent(PacketNumber(0), at(0), 100, None, &mut reno);
    packets.on_sent(
        PacketNumber(1),
        at(10),
        100,
        Some(PayloadToken(0)),
        &mut reno,
    );
    let mut rtt = RttEstimator::default();
    let mut fates = Vec::new();
    packets.on_ack(ack(1, 0b1), at(20), &mut rtt, &mut reno, &mut fates);
    assert_eq!(fates, [PayloadFate::Delivered(PayloadToken(0))]);
    assert_eq!(reno.bytes_in_flight(), 0);
    assert_eq!(size_of::<SentPacket>(), 32);
}
