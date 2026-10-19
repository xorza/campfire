use super::*;

fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap()
}

fn rate(hz: u32) -> TickRate {
    TickRate::try_from(hz).unwrap()
}

fn ticks(text: &str) -> Num {
    text.parse().unwrap()
}

/// 42 ticks in 2 s are 21 Hz; 21 ticks in 2 s, half of 21 Hz, are no whole rate: 2 s are 21
/// ticks, and a tick lasts 2 · 10⁹ / 21 = 95,238,095.2… ns, to the nearest 95,238,095.
#[test]
fn a_rate_is_kept_in_lowest_terms_and_converts_exactly() {
    let doubled = TickRate::new(nonzero(42), nonzero(2));
    assert_eq!(doubled, TickRate::hertz(nonzero(21)));
    assert_eq!(doubled.hz(), Some(nonzero(21)));
    let half = TickRate::new(nonzero(21), nonzero(2));
    assert_eq!((half.ticks(), half.seconds()), (nonzero(21), nonzero(2)));
    assert_eq!(half.hz(), None);
    assert_eq!(half.period(), Duration::from_nanos(95_238_095));
    assert_eq!(half.ticks_in(Duration::from_secs(2)), Num::int(21));
    assert_eq!(half.ticks_covering(Duration::from_secs(2)), 21);
}

/// At 60 Hz the period rounds 16,666,666.6… ns to 16,666,667 ns, but 3 ticks are exactly 50 ms,
/// where 3 rounded periods are 50,000,001 ns, and 1 s is exactly 60 ticks. At 30 Hz the period
/// rounds 33,333,333.3… ns down, and 2 ticks every 3 s last 1.5 s exactly.
#[test]
fn a_rate_rounds_once() {
    let sixty = rate(60);
    assert_eq!(sixty.period(), Duration::from_nanos(16_666_667));
    assert_eq!(sixty.duration_of(Num::int(3)), Duration::from_millis(50));
    assert_eq!(sixty.ticks_in(Duration::from_secs(1)), Num::int(60));
    assert_eq!(rate(30).period(), Duration::from_nanos(33_333_333));
    assert_eq!(
        TickRate::new(nonzero(2), nonzero(3)).period(),
        Duration::from_millis(1500)
    );
}

/// 3.5 ticks at 3 Hz are 1.1666… s, which rounds to 1,166,666,667 ns; 1,906,250 µs at 8 Hz are
/// 15.25 ticks. 1 ms at 60 Hz is 0.06 ticks, 1,006,632.96 steps of 2^-24, to the nearest
/// 1,006,633. A tie goes to the even step: 2·10⁹ ticks a second last 0.5 ns, to 0, and 2·10⁹
/// ticks every 3 s last 1.5 ns, to 2.
#[test]
fn a_conversion_rounds_to_the_nearest_unit() {
    assert_eq!(
        rate(3).duration_of(ticks("3.5")),
        Duration::from_nanos(1_166_666_667)
    );
    assert_eq!(
        rate(8).ticks_in(Duration::from_micros(1_906_250)),
        ticks("15.25")
    );
    assert_eq!(
        rate(60).ticks_in(Duration::from_millis(1)),
        Num::from_bits(1_006_633)
    );
    assert_eq!(rate(2_000_000_000).period(), Duration::ZERO);
    assert_eq!(
        rate(2_000_000_000).period(),
        rate(2_000_000_000).duration_of(Num::ONE)
    );
    assert_eq!(
        TickRate::new(nonzero(2_000_000_000), nonzero(3)).period(),
        Duration::from_nanos(2)
    );
}

/// At 60 Hz, 50 ms are 3 whole ticks, and 1 ns more takes a 4th.
#[test]
fn whole_ticks_cover_a_duration_exactly() {
    let sixty = rate(60);
    assert_eq!(sixty.ticks_covering(Duration::from_millis(50)), 3);
    assert_eq!(
        sixty.ticks_covering(Duration::from_millis(50) + Duration::from_nanos(1)),
        4
    );
    assert_eq!(sixty.ticks_covering(Duration::ZERO), 0);
}

/// 2^24 + 0.5 ticks at 1000 Hz: `f32` holds 2^24 + 0.5 as 2^24, losing the half tick.
#[test]
fn a_conversion_stays_exact_past_the_precision_of_f32() {
    let thousand = rate(1000);
    let ticks = Num::int(16_777_216) + Num::HALF;
    let duration = Duration::from_micros(16_777_216_500);
    assert_eq!(thousand.duration_of(ticks), duration);
    assert_eq!(thousand.ticks_in(duration), ticks);
}

/// The largest number of ticks, 2^39 - 2^-24, at 1 Hz is 2^39 s less 59.6 ns, to the nearest
/// 2^39 s less 60 ns; a duration past the largest number saturates.
#[test]
fn a_conversion_saturates() {
    assert_eq!(
        rate(1).duration_of(Num::MAX),
        Duration::from_secs(1 << 39) - Duration::from_nanos(60)
    );
    assert_eq!(rate(1).ticks_in(Duration::MAX), Num::MAX);
    assert_eq!(
        rate(u32::MAX).ticks_in(Duration::from_secs(1 << 20)),
        Num::MAX
    );
}
