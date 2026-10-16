use super::*;

#[test]
fn a_fan_turns_each_projectile_by_its_share_of_the_spread_rounded_once() {
    let fan = |count, spread| Fan {
        count: NonZeroU8::new(count).unwrap(),
        spread_deg: Num::from_int(spread).unwrap(),
    };
    // π is 52707179 / 2²⁴. Three over 30° turn −15°, 0 and 15°: π / 12 is 4392264.92, to
    // 4392265. Rounded twice, as π / 180 = 292817.66 to 292818 times 15, it was 4392270.
    assert_eq!(Num::PI.to_bits(), 52_707_179);
    let three = [0, 1, 2].map(|at| fan(3, 30).turn(at).to_bits());
    assert_eq!(three, [-4_392_265, 0, 4_392_265]);
    // Two over 90° turn ∓45°: π / 4 is 13176794.75, to 13176795. One turns none.
    let two = [0, 1].map(|at| fan(2, 90).turn(at).to_bits());
    assert_eq!(two, [-13_176_795, 13_176_795]);
    assert_eq!(fan(1, 90).turn(0), Num::ZERO);
    // Four over a full turn are a ring, 0, 90°, 180° and 270° on from the aim, so the first and
    // the last fly apart: π / 2 is 26353589.5, to 26353590 by the even; 3π / 2 is 79060768.5,
    // to 79060768.
    let ring = [0, 1, 2, 3].map(|at| fan(4, 360).turn(at).to_bits());
    assert_eq!(ring, [0, 26_353_590, 52_707_179, 79_060_768]);
}
