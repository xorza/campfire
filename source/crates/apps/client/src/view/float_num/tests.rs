use super::*;

#[test]
fn a_drawn_value_is_the_nearest_number() {
    let bits = |value: f32| to_num(value).map(Num::to_bits);
    // 1234.5677 in `f32` is 1234.5677490234375, 20,712,609,792 steps of 2⁻²⁴; −0.625 is
    // 10,485,760 steps below 0.
    assert_eq!(bits(1234.5677), Some(20_712_609_792));
    assert_eq!(bits(-0.625), Some(-10_485_760));
    // 2⁻²⁵ is half a step and rounds to the even 0; 1.5 · 2⁻²⁴ to the even 2; 1.25 · 2⁻²⁴
    // to 1; −1.5 · 2⁻²⁴ to −2; the smallest subnormal and −0 to 0.
    let step = f32::from_bits(0x3380_0000);
    assert_eq!(bits(step / 2.0), Some(0));
    assert_eq!(bits(step * 1.5), Some(2));
    assert_eq!(bits(step * 1.25), Some(1));
    assert_eq!(bits(step * -1.5), Some(-2));
    assert_eq!(bits(f32::from_bits(1)), Some(0));
    assert_eq!(bits(-0.0), Some(0));
    // −2³⁹ is the least `Num`; 2³⁹ is one past the largest, as are the larger and the values
    // not finite.
    let edge = f32::from_bits(0x5300_0000);
    assert_eq!(bits(-edge), Some(i64::MIN));
    assert_eq!(bits(edge), None);
    assert_eq!(bits(f32::MAX), None);
    assert_eq!(bits(f32::NAN), None);
    assert_eq!(bits(f32::NEG_INFINITY), None);
}

#[test]
fn a_number_draws_as_its_value() {
    // 2.5 and −0.75 are sums of powers of two, exact in an f32.
    assert_eq!(Num::from_bits(5 << (Num::FRAC_BITS - 1)).float(), 2.5);
    assert_eq!(Num::from_bits(-3 << (Num::FRAC_BITS - 2)).float(), -0.75);
    assert_eq!(Num::ZERO.float(), 0.0);
}

#[test]
fn a_point_is_a_sim_place_within_the_bound() {
    // Exactly, its height among its axes: 0.1 in `f32` is 13,421,773 · 2⁻²⁷, which rounds to
    // the step 1,677,722 of 2⁻²⁴, not a millimeter's multiple.
    assert_eq!(
        position(Vec3::new(3.0, 0.0, -2.0)),
        Some(SimVec3::from((3, 0, -2)))
    );
    let tenth = position(Vec3::new(0.1, 0.0, 0.0)).unwrap();
    assert_eq!(tenth.x.to_bits(), 1_677_722);
    // The bound is 2²⁰ on each axis: at it a place, one past it none, though a `Num` holds it;
    // past what a `Num` holds, none.
    assert_eq!(
        position(Vec3::new(0.0, -1_048_576.0, 0.0)),
        Some(SimVec3::from((0, -1_048_576, 0)))
    );
    assert_eq!(position(Vec3::new(0.0, 0.0, 1_048_577.0)), None);
    assert_eq!(position(Vec3::new(1e15, 0.0, 0.0)), None);
    // A ground point drops the height, and holds what a `Num` holds, past the bound too.
    assert_eq!(
        ground_point(Vec3::new(1_048_577.0, 7.0, -2.0)),
        Some(GroundPoint::new(Num::from(1_048_577), Num::from(-2)))
    );
    assert_eq!(ground_point(Vec3::new(f32::NAN, 0.0, 0.0)), None);
}
