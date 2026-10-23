use super::*;

/// Before the peer accepted a packet, each tick goes whole; once it accepted the one of tick
/// 100, tick 101 goes in 2 bytes, its low 15 bits. The overstep follows, its top 8 bits: a half
/// tick is 128.
#[test]
fn a_tick_goes_short_once_the_peer_accepted_one() {
    let mut encoder = TickEncoder::default();
    assert_eq!(
        encoder.encode(TickInstant::from(NetTick(100))).as_bytes(),
        [0x80, 0, 0, 0, 100, 0]
    );
    encoder.on_sent(NetTick(100));
    assert_eq!(
        encoder.encode(TickInstant::from(NetTick(101))).len,
        EncodedTick::MAX_LEN
    );
    encoder.on_delivered(NetTick(100));
    assert_eq!(
        encoder.encode(TickInstant::from(NetTick(101))).as_bytes(),
        [0, 101, 0]
    );
    // 0x1_8005 keeps its low 15 bits, 0x0005, but is too far from 100 to go short.
    assert_eq!(
        encoder.encode(TickInstant::from(NetTick(0x1_8005))).len,
        EncodedTick::MAX_LEN
    );
}

/// A tick within 2^14 of the accepted one goes short, and one 2^14 away goes whole; a sent
/// tick far past the accepted one sends the next whole too, as the receiver may read it.
#[test]
fn a_tick_goes_short_only_near_the_accepted_and_the_sent() {
    let mut encoder = TickEncoder::default();
    encoder.on_sent(NetTick(20_000));
    encoder.on_delivered(NetTick(20_000));
    assert_eq!(
        encoder
            .encode(TickInstant::from(NetTick(20_000 + 16_383)))
            .len,
        SHORT_TICK_LEN + OVERSTEP_LEN
    );
    assert_eq!(
        encoder
            .encode(TickInstant::from(NetTick(20_000 + 16_384)))
            .len,
        EncodedTick::MAX_LEN
    );
    assert_eq!(
        encoder
            .encode(TickInstant::from(NetTick(20_000 - 16_383)))
            .len,
        SHORT_TICK_LEN + OVERSTEP_LEN
    );
    encoder.on_sent(NetTick(40_000));
    assert_eq!(
        encoder.encode(TickInstant::from(NetTick(23_000))).len,
        EncodedTick::MAX_LEN
    );
    assert_eq!(
        encoder.encode(TickInstant::from(NetTick(30_000))).len,
        SHORT_TICK_LEN + OVERSTEP_LEN
    );
}

/// The delivered tick only grows: a late delivery of an older packet, of tick 10 after 11,
/// leaves 11.
#[test]
fn a_late_delivery_keeps_the_largest_tick() {
    let mut encoder = TickEncoder::default();
    encoder.on_delivered(NetTick(11));
    encoder.on_delivered(NetTick(10));
    assert_eq!(encoder.delivered, Some(NetTick(11)));
}
