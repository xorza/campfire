use super::*;
use core::cmp::Ordering;

fn num(text: &str) -> Num {
    text.parse().unwrap()
}

fn instant(text: &str) -> TickInstant {
    TickInstant::try_from(num(text)).unwrap()
}

fn span(text: &str) -> TickSpan {
    TickSpan::try_from(num(text)).unwrap()
}

fn positive(text: &str) -> PositiveTickSpan {
    PositiveTickSpan::try_from(num(text)).unwrap()
}

fn rate(hz: u32) -> TickRate {
    TickRate::try_from(hz).unwrap()
}

#[test]
fn an_instant_orders_by_its_tick_then_its_overstep() {
    let t1 = instant("10.5");
    let t2 = instant("10.7");
    let t3 = instant("11.2");

    assert!(t1 < t2);
    assert!(t2 < t3);
    assert!(t1 < t3);

    assert_eq!(t1.cmp(&t1), Ordering::Equal);
    assert_eq!(t1.cmp(&t2), Ordering::Less);
    assert_eq!(t2.cmp(&t1), Ordering::Greater);
}

/// 10.7 ticks are 179,516,211.2 steps of 2^-24, to the nearest 179,516,211: tick 10 and the
/// overstep's 11,744,051 steps.
#[test]
fn an_instant_splits_into_its_tick_and_its_overstep() {
    let at = instant("10.7");
    assert_eq!(at.value(), Num::from_bits(179_516_211));
    assert_eq!(at.tick(), NetTick(10));
    assert_eq!(at.overstep().value(), Num::from_bits(11_744_051));
    assert_eq!(
        TickInstant::from_tick_and_overstep(at.tick(), at.overstep()),
        at
    );
}

// Every literal below is a multiple of 1/4, which the binary fixed-point value holds exactly, so
// each sum and difference is exact.

#[test]
fn adding_a_positive_span_moves_forward() {
    assert_eq!(instant("10.25") + span("5.25"), instant("15.5"));
}

#[test]
fn adding_a_negative_span_moves_back() {
    assert_eq!(instant("10.25") + span("-5.25"), instant("5"));
}

#[test]
fn adding_carries_an_overstep_overflow_into_the_tick() {
    // 0.75 + 0.5 = 1.25, which is 1 tick + 0.25 overstep
    assert_eq!(instant("10.75") + span("5.5"), instant("16.25"));
}

#[test]
fn subtracting_a_positive_span_moves_back() {
    assert_eq!(instant("10.75") - span("5.25"), instant("5.5"));
}

#[test]
fn subtracting_a_negative_span_moves_forward() {
    assert_eq!(instant("10.25") - span("-5.25"), instant("15.5"));
}

#[test]
fn subtracting_borrows_an_overstep_underflow_from_the_tick() {
    // 0.25 - 0.75 = -0.5, which becomes 0.5 with borrowing from tick
    assert_eq!(instant("10.25") - span("5.75"), instant("4.5"));
}

#[test]
fn subtracting_instants_gives_their_span() {
    let t1 = instant("15.75");
    let t2 = instant("10.25");
    assert_eq!(t1 - t2, span("5.5"));
    assert_eq!(t2 - t1, span("-5.5"));
    // A borrow from the tick.
    assert_eq!(instant("15.25") - instant("10.75"), span("4.5"));
}

#[test]
fn a_duration_round_trips_through_ticks() {
    // 15.25 ticks at 8 Hz are 1.90625 s: `Duration` holds every step exactly.
    let original = instant("15.25");

    let duration = original.to_duration(rate(8));
    assert_eq!(duration, Duration::from_micros(1_906_250));
    assert_eq!(TickInstant::from_duration(duration, rate(8)), original);
}

/// The last instant is 2^32 - 2^-24 ticks, and an instant's difference saturates at the signed
/// range, `[-2^31, 2^31 - 2^-24]`.
#[test]
fn arithmetic_saturates_at_the_unsigned_range() {
    let last = TickInstant::try_from(Num::from_bits(UNSIGNED_MAX)).unwrap();
    assert_eq!(last.tick(), NetTick(u32::MAX));
    assert_eq!(last + TickSpan::from(1), last);
    assert_eq!(TickInstant::ZERO - TickSpan::from(1), TickInstant::ZERO);
    assert_eq!(TickInstant::ZERO + TickSpan::from(-1), TickInstant::ZERO);
    assert_eq!(last - TickInstant::ZERO, TickSpan::MAX);
    assert_eq!(TickInstant::ZERO - last, TickSpan::MIN);
    assert_eq!(
        TickInstant::try_from(Num::from_bits(UNSIGNED_MAX + 1)),
        Err(TickRangeError)
    );
}

#[test]
fn a_span_has_the_sign_of_its_value() {
    for (text, positive, negative) in [
        ("-32768", false, true),
        ("-32767.75", false, true),
        ("0", false, false),
        ("32767.75", true, false),
    ] {
        let span = span(text);
        assert_eq!(
            (span.is_positive(), span.is_negative()),
            (positive, negative)
        );
    }
}

#[test]
fn negation_flips_the_sign() {
    let five = span("5.25");
    let negated = -five;
    assert_eq!(negated, span("-5.25"));
    assert_eq!(-negated, five);
}

#[test]
fn a_signed_addition_carries_and_borrows() {
    let ten = TickSpan::from(10);

    assert_eq!((ten + ten).to_i32(), 20);
    assert_eq!((ten + (-ten)).to_i32(), 0);
    assert_eq!(((-ten) + ten).to_i32(), 0);
    assert_eq!(((-ten) + (-ten)).to_i32(), -20);
}

/// A half rounds away from zero, and the largest span, 2^31 - 2^-24, rounds to 2^31, past
/// `i32`, so it saturates; rounded up, any fraction takes the next tick above.
#[test]
fn to_i32_rounds_to_the_nearest_tick_or_up() {
    assert_eq!(span("0.49").to_i32(), 0);
    assert_eq!(span("0.5").to_i32(), 1);
    assert_eq!(span("0.61").to_i32(), 1);

    assert_eq!(span("-0.49").to_i32(), 0);
    assert_eq!(span("-0.5").to_i32(), -1);
    assert_eq!(span("-0.61").to_i32(), -1);

    assert_eq!(TickSpan::MAX.to_i32(), i32::MAX);
    assert_eq!(TickSpan::MIN.to_i32(), i32::MIN);
    assert_eq!(
        ["0.01", "0.5", "1", "-0.01", "-0.99", "-1"].map(|text| span(text).to_i32_ceil()),
        [1, 1, 1, 0, 0, -1]
    );
    assert_eq!(TickSpan::MAX.to_i32_ceil(), i32::MAX);
}

fn mul(span: TickSpan, numerator: u32, denominator: u32) -> TickSpan {
    span.mul_ratio(numerator, NonZeroU32::new(denominator).unwrap())
}

#[test]
fn a_multiplication_scales_the_tick_and_the_overstep() {
    let ten_and_a_half = span("10.5");
    assert_eq!(mul(ten_and_a_half, 2, 1), span("21"));
    assert_eq!(mul(ten_and_a_half, 3, 2), span("15.75"));
    // The overstep carries into the tick: 0.75 · 1.5 = 1.125.
    assert_eq!(mul(span("10.75"), 3, 2), span("16.125"));

    // 10 · 1/50 = 0.2 is 3,355,443.2 steps of 2^-24, which round down; 10 · 3/50 = 0.6 is
    // 10,066,329.6, which round up; the literals round to the nearest too.
    let ten = TickSpan::from(10);
    assert_eq!(mul(ten, 1, 50).value(), Num::from_bits(3_355_443));
    assert_eq!(mul(ten, 1, 50), span("0.2"));
    assert_eq!(mul(ten, 3, 50).value(), Num::from_bits(10_066_330));
    assert_eq!(mul(ten, 3, 50), span("0.6"));
    // A tie goes to the even step: ±2^-24 · 1/2 to 0, and 3 · 2^-24 · 1/2 to 2 · 2^-24.
    let step = TickSpan::try_from(Num::EPSILON).unwrap();
    assert_eq!(mul(-step, 1, 2), TickSpan::ZERO);
    assert_eq!(mul(step, 1, 2), TickSpan::ZERO);
    assert_eq!(mul(step, 3, 2), step + step);
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
        assert_eq!(span(a) - span(b), span(expected), "{a} - {b}");
    }
}

/// A span holds up to 2^31 - 2^-24 ticks: one tick count or one 2^-24 step past that is refused,
/// where a cast made it a negative span.
#[test]
fn conversions_refuse_values_outside_the_signed_range() {
    let largest = Num::from_bits(SIGNED_MAX);
    let past = Num::int(1 << 31);
    assert_eq!(
        TickSpan::try_from(NetTick(i32::MAX.cast_unsigned())),
        Ok(TickSpan::from(i32::MAX))
    );
    assert_eq!(TickSpan::try_from(NetTick(1 << 31)), Err(TickRangeError));
    assert_eq!(
        TickSpan::try_from(PositiveTickSpan::try_from(largest).unwrap()),
        Ok(TickSpan::MAX)
    );
    assert_eq!(
        TickSpan::try_from(PositiveTickSpan::try_from(past).unwrap()),
        Err(TickRangeError)
    );
    assert_eq!(
        TickSpan::try_from(TickInstant::try_from(largest).unwrap()),
        Ok(TickSpan::MAX)
    );
    assert_eq!(
        TickSpan::try_from(TickInstant::try_from(past).unwrap()),
        Err(TickRangeError)
    );

    assert_eq!(
        PositiveTickSpan::try_from(span("0")),
        Ok(PositiveTickSpan::default())
    );
    assert_eq!(
        PositiveTickSpan::try_from(span("-0.25")),
        Err(TickRangeError)
    );
    assert_eq!(TickInstant::try_from(span("2.5")), Ok(instant("2.5")));
    assert_eq!(TickInstant::try_from(span("-0.25")), Err(TickRangeError));
}

#[test]
fn arithmetic_saturates_at_the_signed_range() {
    let (max, min) = (TickSpan::MAX, TickSpan::MIN);
    assert_eq!(max + TickSpan::from(1), max);
    assert_eq!(min - TickSpan::from(1), min);
    assert_eq!(-min, max);
    // A ratio above 1 saturates at both ends.
    assert_eq!(mul(max, 2, 1), max);
    assert_eq!(mul(min, 2, 1), min);
    // The size of the smallest span, -2^31, is exact.
    assert_eq!(
        min.unsigned_abs(),
        PositiveTickSpan::try_from(Num::int(1 << 31)).unwrap()
    );
    assert_eq!(span("-2.5").unsigned_abs(), positive("2.5"));
}

/// The ticks of a duration are exact: 1,906.25 ms at 8 Hz are 15.25 ticks, and one second past
/// the signed range saturates at its largest value.
#[test]
fn a_duration_converts_exactly_and_saturates() {
    assert_eq!(
        TickSpan::from_duration(Duration::from_micros(1_906_250), rate(8)),
        span("15.25")
    );
    assert_eq!(
        TickSpan::from_duration(Duration::from_secs(1 << 31), rate(1)),
        TickSpan::MAX
    );
}

/// A peer's frame time of 2^31 ticks was a negative span, whose duration panicked; as an
/// unsigned span at 50 Hz it is 2^31 · 20 ms, exactly, and 50 ms are 2.5 ticks.
#[test]
fn to_duration_takes_any_unsigned_tick_count() {
    let long = PositiveTickSpan::from_ticks(1 << 31);
    assert_eq!(long.to_duration(rate(50)), Duration::from_millis(20 << 31));
    assert_eq!(
        PositiveTickSpan::from_duration(Duration::from_millis(50), rate(50)),
        positive("2.5")
    );
}

/// 2.6 ticks are 43,620,761.6 steps, to the nearest 43,620,762: 2 whole ticks and an overstep of
/// 10,066,330 steps, whose top 8 bits, 153, go on the wire, so the span reads back as 2 and
/// 153/256 ticks.
#[test]
fn a_span_travels_as_its_whole_ticks_and_its_overstep_byte() {
    let sent = positive("2.6");
    assert_eq!(sent.value(), Num::from_bits(43_620_762));
    assert_eq!((sent.whole_ticks(), sent.overstep().to_u8()), (2, 153));
    let mut bytes = Vec::new();
    sent.to_bytes(&mut bytes);
    assert_eq!(bytes, [0, 0, 0, 2, 153]);
    let read = PositiveTickSpan::from_bytes(&mut Reader::from(bytes)).unwrap();
    assert_eq!(read.value(), Num::int(2) + Num::from_bits(153 << 16));
}
