use super::*;

#[test]
fn a_ring_widens_to_1_8_times_its_reach_then_goes() {
    // From 10 h of app time, of a reach of 2: 2 wide at its start, 2 · (1 + 0.8 × 0.5) = 2.8 at
    // half its 350 ms, 2 · (1 + 0.8 × 0.99) = 3.584 just before its end, and gone at its end.
    // Each width comes from a difference of times, in f32 to some ulps of 2, below 1e-6.
    let since = Duration::from_secs(36_000);
    let ring = Ring { since, reach: 2.0 };
    let at = |ms| ring.scale(since + Duration::from_millis(ms));
    assert_eq!(at(0), Some(2.0));
    let halfway = at(175).unwrap();
    assert!((halfway - 2.8).abs() < 1e-6, "{halfway}");
    let late = ring.scale(since + RING_TIME.mul_f64(0.99)).unwrap();
    assert!((late - 3.584).abs() < 1e-6, "{late}");
    assert_eq!(at(350), None);
    // A time before its start, as a clock never gives, holds it at its first width.
    assert_eq!(ring.scale(Duration::ZERO), Some(2.0));
}
