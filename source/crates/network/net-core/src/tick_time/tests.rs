use super::*;
use core::cmp::Ordering;

#[test]
fn instants_order_by_tick_then_overstep() {
    let t1 = TickInstant::lit("10.5");
    let t2 = TickInstant::lit("10.7");
    let t3 = TickInstant::lit("11.2");

    assert!(t1 < t2);
    assert!(t2 < t3);
    assert!(t1 < t3);

    assert_eq!(t1.cmp(&t1), Ordering::Equal);
    assert_eq!(t1.cmp(&t2), Ordering::Less);
    assert_eq!(t2.cmp(&t1), Ordering::Greater);
}

// Every literal is a multiple of 1/4, which the binary fixed-point value holds exactly, so each
// sum and difference is exact.

#[test]
fn adding_a_positive_delta_moves_forward() {
    let result = TickInstant::lit("10.25") + TickDelta::lit("5.25");
    assert_eq!(result, TickInstant::lit("15.5"));
}

#[test]
fn adding_a_negative_delta_moves_back() {
    let result = TickInstant::lit("10.25") + TickDelta::lit("-5.25");
    assert_eq!(result, TickInstant::lit("5"));
}

#[test]
fn adding_carries_an_overstep_overflow_into_the_tick() {
    // 0.75 + 0.5 = 1.25, which is 1 tick + 0.25 overstep
    let result = TickInstant::lit("10.75") + TickDelta::lit("5.5");
    assert_eq!(result, TickInstant::lit("16.25"));
}

#[test]
fn subtracting_a_positive_delta_moves_back() {
    let result = TickInstant::lit("10.75") - TickDelta::lit("5.25");
    assert_eq!(result, TickInstant::lit("5.5"));
}

#[test]
fn subtracting_a_negative_delta_moves_forward() {
    let result = TickInstant::lit("10.25") - TickDelta::lit("-5.25");
    assert_eq!(result, TickInstant::lit("15.5"));
}

#[test]
fn subtracting_borrows_an_overstep_underflow_from_the_tick() {
    // 0.25 - 0.75 = -0.5, which becomes 0.5 with borrowing from tick
    let result = TickInstant::lit("10.25") - TickDelta::lit("5.75");
    assert_eq!(result, TickInstant::lit("4.5"));
}

#[test]
fn subtracting_instants_gives_their_delta() {
    let t1 = TickInstant::lit("15.75");
    let t2 = TickInstant::lit("10.25");
    assert_eq!(t1 - t2, TickDelta::lit("5.5"));
    assert_eq!(t2 - t1, TickDelta::lit("-5.5"));
}

#[test]
fn subtracting_instants_borrows_an_overstep_underflow() {
    // Need to borrow from tick
    let delta = TickInstant::lit("15.25") - TickInstant::lit("10.75");
    assert_eq!(delta, TickDelta::lit("4.5"));
}

#[test]
fn a_duration_round_trips_through_ticks() {
    // 15.25 ticks at 8 Hz are 1.90625 s: `Duration` holds every step exactly.
    let rate = TickRate::new(core::num::NonZeroU16::new(8).unwrap());
    let original = TickInstant::lit("15.25");

    let duration = original.to_duration(rate);
    assert_eq!(duration, Duration::from_micros(1_906_250));
    assert_eq!(TickInstant::from_duration(duration, rate), original);
}

#[test]
fn arithmetic_saturates_at_the_unsigned_range() {
    use fixed::types::{I32F32, U32F32};

    let last = TickInstant { value: U32F32::MAX };
    assert_eq!(last + TickDelta::from(1), last);
    assert_eq!(
        TickInstant::zero() - TickDelta::from(1),
        TickInstant::zero()
    );
    assert_eq!(
        TickInstant::zero() + TickDelta::from(-1),
        TickInstant::zero()
    );
    assert_eq!(last - TickInstant::zero(), TickDelta::new(I32F32::MAX));
    assert_eq!(TickInstant::zero() - last, TickDelta::new(I32F32::MIN));
}

#[test]
fn accessors_split_a_delta_into_tick_and_overstep() {
    let delta = TickDelta::lit("-32768");
    assert!(!delta.is_positive());
    assert!(delta.is_negative());

    let delta = TickDelta::lit("-32767.75");
    assert!(!delta.is_positive());
    assert!(delta.is_negative());

    let delta = TickDelta::lit("32767.75");
    assert!(delta.is_positive());
    assert!(!delta.is_negative());
}

#[test]
fn negation_flips_the_sign() {
    let delta = TickDelta::lit("5.25");
    let negated = -delta;
    assert_eq!(negated, TickDelta::lit("-5.25"));
    assert_eq!(-negated, delta);
}

#[test]
fn signed_addition_carries_and_borrows() {
    let delta = TickDelta::from(10);

    assert_eq!((delta + delta).to_i32(), 20);
    assert_eq!((delta + (-delta)).to_i32(), 0);
    assert_eq!(((-delta) + delta).to_i32(), 0);
    assert_eq!(((-delta) + (-delta)).to_i32(), -20);
}

#[test]
fn to_i32_rounds_to_the_nearest_tick() {
    assert_eq!(TickDelta::lit("0.49").to_i32(), 0);
    assert_eq!(TickDelta::lit("0.5").to_i32(), 1);
    assert_eq!(TickDelta::lit("0.61").to_i32(), 1);

    assert_eq!(TickDelta::lit("-0.49").to_i32(), 0);
    assert_eq!(TickDelta::lit("-0.5").to_i32(), -1);
    assert_eq!(TickDelta::lit("-0.61").to_i32(), -1);
}

fn mul(delta: TickDelta, numerator: u32, denominator: u32) -> TickDelta {
    delta.mul_ratio(numerator, NonZeroU32::new(denominator).unwrap())
}

#[test]
fn multiplication_scales_tick_and_overstep() {
    use fixed::types::I32F32;

    let delta = TickDelta::lit("10.5");
    assert_eq!(mul(delta, 2, 1), TickDelta::lit("21"));
    assert_eq!(mul(delta, 3, 2), TickDelta::lit("15.75"));
    // The overstep carries into the tick: 0.75 · 1.5 = 1.125.
    assert_eq!(mul(TickDelta::lit("10.75"), 3, 2), TickDelta::lit("16.125"));

    // 10 · 1/50 = 0.2 is 858_993_459.2 units of 2^-32, which round down; 10 · 3/50 = 0.6 is
    // 2_576_980_377.6, which round up; the literals round to the nearest too.
    assert_eq!(mul(TickDelta::from(10), 1, 50), TickDelta::lit("0.2"));
    assert_eq!(mul(TickDelta::from(10), 3, 50), TickDelta::lit("0.6"));
    // A half rounds up: -2^-32 · 1/2 to 0, and 2^-32 · 1/2 to 2^-32.
    let unit = TickDelta::new(I32F32::DELTA);
    assert_eq!(mul(-unit, 1, 2), TickDelta::from(0));
    assert_eq!(mul(unit, 1, 2), unit);
}

#[test]
fn subtraction_borrows_across_the_tick() {
    // Multiples of 1/4 keep each difference exact.
    let cases = [
        ("10", "20", "-10"),
        // same tick, rhs > self
        ("0.25", "0.75", "-0.5"),
        // same tick, self > rhs
        ("0.75", "0.25", "0.5"),
        // different tick, no underflow
        ("2.75", "1.25", "1.5"),
        // different tick, underflow
        ("2.25", "1.75", "0.5"),
        // rhs > self, no underflow
        ("1.25", "2.75", "-1.5"),
        // rhs > self, underflow
        ("1.75", "2.25", "-0.5"),
    ];
    for (a, b, expected) in cases {
        assert_eq!(
            TickDelta::lit(a) - TickDelta::lit(b),
            TickDelta::lit(expected),
            "{a} - {b}"
        );
    }
}

/// `I32F32` holds up to 2^31 - 2^-32 ticks: one tick count or one 2^-32 step past that is refused,
/// where a cast made it a negative delta.
#[test]
fn conversions_refuse_values_outside_the_signed_range() {
    use crate::net_tick::NetTick;
    use crate::tick_range_error::TickRangeError;
    use crate::tick_time::PositiveTickDelta;
    use crate::tick_time::TickInstant;
    use fixed::types::{I32F32, U32F32};

    let largest =
        U32F32::from_bits((u64::from(i32::MAX.cast_unsigned()) << 32) | u64::from(u32::MAX));
    let past = U32F32::from_num(1_u64 << 31);
    assert_eq!(
        TickDelta::try_from(NetTick(i32::MAX.cast_unsigned())),
        Ok(TickDelta::from(i32::MAX))
    );
    assert_eq!(TickDelta::try_from(NetTick(1 << 31)), Err(TickRangeError));
    assert_eq!(
        TickDelta::try_from(PositiveTickDelta { value: largest }),
        Ok(TickDelta::new(I32F32::MAX))
    );
    assert_eq!(
        TickDelta::try_from(PositiveTickDelta { value: past }),
        Err(TickRangeError)
    );
    assert_eq!(
        TickDelta::try_from(TickInstant { value: largest }),
        Ok(TickDelta::new(I32F32::MAX))
    );
    assert_eq!(
        TickDelta::try_from(TickInstant { value: past }),
        Err(TickRangeError)
    );

    assert_eq!(
        PositiveTickDelta::try_from(TickDelta::lit("0")),
        Ok(PositiveTickDelta::lit("0"))
    );
    assert_eq!(
        PositiveTickDelta::try_from(TickDelta::lit("-0.25")),
        Err(TickRangeError)
    );
    assert_eq!(
        TickInstant::try_from(TickDelta::lit("2.5")),
        Ok(TickInstant::lit("2.5"))
    );
    assert_eq!(
        TickInstant::try_from(TickDelta::lit("-0.25")),
        Err(TickRangeError)
    );
}

#[test]
fn arithmetic_saturates_at_the_signed_range() {
    use fixed::types::{I32F32, U32F32};

    let max = TickDelta::new(I32F32::MAX);
    let min = TickDelta::new(I32F32::MIN);
    assert_eq!(max + TickDelta::from(1), max);
    assert_eq!(min - TickDelta::from(1), min);
    assert_eq!(-min, max);
    // A ratio above 1 saturates at both ends.
    assert_eq!(mul(max, 2, 1), max);
    assert_eq!(mul(min, 2, 1), min);
    // The size of the smallest delta, -2^31, is exact.
    assert_eq!(
        min.unsigned_abs(),
        PositiveTickDelta {
            value: U32F32::from_num(1_u64 << 31)
        }
    );
    assert_eq!(
        TickDelta::lit("-2.5").unsigned_abs(),
        PositiveTickDelta::lit("2.5")
    );
}

/// The ticks of a duration are exact: 1,906.25 ms at 8 Hz are 15.25 ticks, and one second past
/// `I32F32`'s range saturates at its largest value.
#[test]
fn from_duration_is_exact_and_saturates() {
    use core::num::NonZeroU16;
    use core::time::Duration;
    use fixed::types::I32F32;

    let rate = |hz| TickRate::new(NonZeroU16::new(hz).unwrap());
    assert_eq!(
        TickDelta::from_duration(Duration::from_micros(1_906_250), rate(8)),
        TickDelta::lit("15.25")
    );
    assert_eq!(
        TickDelta::from_duration(Duration::from_secs(1 << 31), rate(1)),
        TickDelta::new(I32F32::MAX)
    );
}

/// A peer's frame time of 2^31 ticks was a negative `TickDelta`, whose duration panicked; as
/// an unsigned delta at 50 Hz it is 2^31 · 20 ms, exactly, and 50 ms are 2.5 ticks.
#[test]
fn to_duration_takes_any_unsigned_tick_count() {
    let rate = TickRate::new(core::num::NonZeroU16::new(50).unwrap());
    let delta = PositiveTickDelta {
        value: U32F32::from_num(1_u64 << 31),
    };
    assert_eq!(delta.to_duration(rate), Duration::from_millis(20 << 31));
    assert_eq!(
        PositiveTickDelta::from_duration(Duration::from_millis(50), rate),
        PositiveTickDelta::lit("2.5")
    );
}
