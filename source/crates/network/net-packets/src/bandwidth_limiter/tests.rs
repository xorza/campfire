use super::*;
use core::time::Duration;

fn at(millis: u64) -> Instant {
    Instant::from_elapsed(Duration::from_millis(millis))
}

/// The burst is the limiter's own, whatever the MTU: 300 bytes at once, and nothing after
/// them.
#[test]
fn the_bucket_starts_full_at_its_burst() {
    let mut limiter = BandwidthLimiter::new(BandwidthLimit::new(1).with_burst(300), at(0));
    assert_eq!(limiter.available(at(0)), 300);
    limiter.consume(300, at(0));
    assert_eq!(limiter.available(at(0)), 0);
}

/// At 100 B/s with a burst of 300: after 300 bytes at 0 s, 1 s refills 100; half a second
/// after that, 50 more; and 10 s refill only up to the burst.
#[test]
fn the_bucket_refills_at_the_sustained_rate_up_to_the_burst() {
    let mut limiter = BandwidthLimiter::new(BandwidthLimit::new(100).with_burst(300), at(0));
    limiter.consume(300, at(0));
    assert_eq!(limiter.available(at(1_000)), 100);
    limiter.consume(100, at(1_000));
    assert_eq!(limiter.available(at(1_500)), 50);
    assert_eq!(limiter.available(at(11_500)), 300);

    // A rate of 200 B/s refills 200 in the same 1 s, where 100 B/s refilled 100.
    let mut faster = BandwidthLimiter::new(BandwidthLimit::new(200).with_burst(300), at(0));
    faster.consume(300, at(0));
    assert_eq!(faster.available(at(1_000)), 200);
}

/// A packet past what the bucket holds empties it, and the refill starts from there.
#[test]
fn a_packet_past_the_tokens_empties_the_bucket() {
    let mut limiter = BandwidthLimiter::new(BandwidthLimit::new(100).with_burst(300), at(0));
    limiter.consume(1_000, at(0));
    assert_eq!(limiter.available(at(0)), 0);
    assert_eq!(limiter.available(at(10)), 1);
}
