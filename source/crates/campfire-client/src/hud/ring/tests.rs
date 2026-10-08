use super::*;

#[test]
fn a_ring_widens_to_1_8_times_then_goes() {
    // From 10 h of app time: 1 wide at its start, 1 + 0.8 × 0.5 = 1.4 at half its 350 ms,
    // 1 + 0.8 × 0.99 = 1.792 just before its end, and gone at its end. Each width comes from a
    // difference of times, in f32 to some ulps of 1, below 1e-6.
    let since = Duration::from_secs(36_000);
    let ring = Ring { since };
    let at = |ms| ring.scale(since + Duration::from_millis(ms));
    assert_eq!(at(0), Some(1.0));
    let halfway = at(175).unwrap();
    assert!((halfway - 1.4).abs() < 1e-6, "{halfway}");
    let late = ring.scale(since + RING_TIME.mul_f64(0.99)).unwrap();
    assert!((late - 1.792).abs() < 1e-6, "{late}");
    assert_eq!(at(350), None);
    // A time before its start, as a clock never gives, holds it at its first width.
    assert_eq!(ring.scale(Duration::ZERO), Some(1.0));
}
