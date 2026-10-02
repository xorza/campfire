use super::*;

#[test]
fn a_ring_widens_to_1_8_times_then_goes() {
    let ring = Ring { since: 2.0 };
    assert_eq!(ring.scale(2.0), Some(1.0));
    // App times are f32 seconds: a time of 2 s and more holds about 7 digits, so a width
    // read through the time's difference is exact to some ulps of 2, below 1e-5.
    let halfway = ring.scale(2.0 + RING_SECONDS / 2.0).unwrap();
    assert!((halfway - 1.4).abs() < 1e-5, "{halfway}");
    // Just before its end it is nearly 1.8 wide; at its end, gone. A ring from time 0 ends
    // at `RING_SECONDS` exactly, with no difference of times to round.
    let late = ring.scale(2.0 + 0.99 * RING_SECONDS).unwrap();
    assert!((late - 1.792).abs() < 1e-5, "{late}");
    assert_eq!(Ring { since: 0.0 }.scale(RING_SECONDS), None);
}
