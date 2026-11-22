use std::str::FromStr;

use super::*;

/// Zero Hour's speed: a penalty of 1, at least 0.5 and at most 0.8, a share of none for a player
/// of no power, slowing trains and builds.
fn zero_hour() -> PowerSpeedData {
    PowerSpeedData {
        penalty: Share::ALL,
        min: share("0.5"),
        max: share("0.8"),
        empty: EmptyPower::None,
        slows: vec![ActionKind::Train, ActionKind::Build],
    }
}

fn share(text: &str) -> Share {
    Share::from_str(text).unwrap()
}

fn power(given: i64, used: i64) -> PlayerPower {
    PlayerPower::new(Num::from_int(given).unwrap(), Num::from_int(used).unwrap())
}

/// The ticks a time of 150 takes at `speed` for a player of `power`.
fn of_150(speed: PowerSpeed, power: PlayerPower) -> u64 {
    speed.time_at(Ticks::new(150), power).get()
}

#[test]
fn a_short_players_time_is_its_time_over_the_clamped_speed_rounded_down() {
    let speed = PowerSpeed::new(&zero_hour()).unwrap();
    for ((given, used), ticks) in [
        // Not short, or giving some and taking none: the time itself.
        ((5, 5), 150),
        ((6, 5), 150),
        ((5, 0), 150),
        // 4 of 5: 1 − 0.2 = 0.8, at most 0.8; 150 / 0.8 = 187.5, down to 187.
        ((4, 5), 187),
        // 3 of 5: 0.6; 150 / 0.6 = 250.
        ((3, 5), 250),
        // 2 of 3: 2/3, which no decimal holds; 150 × 3 / 2 = 225 exactly.
        ((2, 3), 225),
        // 1 of 5: 0.2, at least 0.5; 150 / 0.5 = 300.
        ((1, 5), 300),
        // 99 of 100: 0.99, at most 0.8 however little short; 187.
        ((99, 100), 187),
        // No power and no use is a share of none in Zero Hour: 0.5, 300.
        ((0, 0), 300),
    ] {
        assert_eq!(
            of_150(speed, power(given, used)),
            ticks,
            "{given} of {used}"
        );
    }
}

#[test]
fn each_rule_of_the_speed_changes_the_time() {
    let base = PowerSpeed::new(&zero_hour()).unwrap();
    // `empty`: a player of no power and no use builds at full speed, 150, not 300.
    let full = PowerSpeed::new(&PowerSpeedData {
        empty: EmptyPower::Full,
        ..zero_hour()
    })
    .unwrap();
    assert_eq!(
        [of_150(base, power(0, 0)), of_150(full, power(0, 0))],
        [300, 150]
    );
    // `penalty`: at 0.5, 2 of 5 is 1 − 0.6 × 0.5 = 0.7, 150 / 0.7 = 214.28, 214; at 1, 0.4, at
    // least 0.5, 300.
    let half = PowerSpeed::new(&PowerSpeedData {
        penalty: share("0.5"),
        ..zero_hour()
    })
    .unwrap();
    assert_eq!(
        [of_150(base, power(2, 5)), of_150(half, power(2, 5))],
        [300, 214]
    );
    // `min`: at 0.25, 1 of 5 is 0.2, at least 0.25: 600, where 0.5 gives 300.
    let low = PowerSpeed::new(&PowerSpeedData {
        min: share("0.25"),
        ..zero_hour()
    })
    .unwrap();
    assert_eq!(
        [of_150(base, power(1, 5)), of_150(low, power(1, 5))],
        [300, 600]
    );
    // `max`: at 1, 99 of 100 is 0.99, 151.5, 151, where 0.8 gives 187; OpenRA's single
    // modifier is a `min` equal to its `max`, 0.5 at any shortage.
    let high = PowerSpeed::new(&PowerSpeedData {
        max: Share::ALL,
        ..zero_hour()
    })
    .unwrap();
    let fixed = PowerSpeed::new(&PowerSpeedData {
        max: share("0.5"),
        ..zero_hour()
    })
    .unwrap();
    assert_eq!(
        [
            of_150(base, power(99, 100)),
            of_150(high, power(99, 100)),
            of_150(fixed, power(99, 100))
        ],
        [187, 151, 300]
    );
    // A fraction of power counts exactly: 0.5 of 1 is 0.5, 300.
    let half_given = PlayerPower::new(Num::HALF, Num::ONE);
    assert_eq!(of_150(base, half_given), 300);
}

#[test]
fn a_speed_slows_the_kinds_it_names_and_refuses_a_broken_rule() {
    let speed = PowerSpeed::new(&zero_hour()).unwrap();
    let slowed = ActionKind::ALL.iter().filter(|&&kind| speed.slows(kind));
    assert_eq!(
        slowed.copied().collect::<Vec<_>>(),
        [ActionKind::Train, ActionKind::Build]
    );
    let refused = |data: PowerSpeedData| PowerSpeed::new(&data).unwrap_err();
    assert_eq!(
        refused(PowerSpeedData {
            min: Share::NONE,
            ..zero_hour()
        }),
        ProductionDataError::PowerMin
    );
    assert_eq!(
        refused(PowerSpeedData {
            min: share("0.9"),
            ..zero_hour()
        }),
        ProductionDataError::PowerMinPastMax
    );
    assert_eq!(
        refused(PowerSpeedData {
            slows: vec![ActionKind::Train, ActionKind::Attack],
            ..zero_hour()
        }),
        ProductionDataError::PowerSlows(ActionKind::Attack)
    );
    // `min` equal to `max` is a rule, OpenRA's.
    assert!(
        PowerSpeed::new(&PowerSpeedData {
            min: share("0.8"),
            ..zero_hour()
        })
        .is_ok()
    );
}

#[test]
fn the_product_and_quotient_are_exact_past_128_bits() {
    // Small values against the product of `u128`.
    for (a, b, c) in [
        (0, 5, 3),
        (7, 0, 3),
        (150, 5, 4),
        (150, 3, 2),
        (1 << 31, 9, 7),
    ] {
        assert_eq!(PowerSpeed::mul_div(a, b, c), u128::from(a) * b / c);
    }
    // (2⁶⁴ − 1) × 7 / 3, of a product past 2⁶⁴, which 3 divides: 43042402838655620435.
    assert_eq!(
        PowerSpeed::mul_div(u64::MAX, 7, 3),
        43_042_402_838_655_620_435
    );
    // (2⁶⁴ − 1) × b / c for c = 2⁹³ + 12345 and b = 1000 c + c − 1, a product past 2¹⁶⁰:
    // 18465190817783261166614.
    let c = (1 << 93) + 12345;
    let b = 1000 * c + (c - 1);
    assert_eq!(
        PowerSpeed::mul_div(u64::MAX, b, c),
        18_465_190_817_783_261_166_614
    );
}

#[test]
fn a_site_grows_in_its_time_at_its_players_power_and_its_slowest_is_its_min() {
    let speed = PowerSpeed::new(&zero_hour()).unwrap();
    let time = Num::int(150);
    // A site of 150 ticks at a share of 0 grows 150 / 300 = 0.5 a tick: 300 ticks, never later.
    let none = power(0, 0);
    let rate = speed.site_rate(Num::ONE, time, none);
    assert_eq!(rate, Num::HALF);
    // At 4 of 5, 187 ticks: 150 / 187 a tick, rounded up, reaches 150 in the 187th tick and not
    // in the 186th.
    let rate = speed.site_rate(Num::ONE, time, power(4, 5));
    assert!(rate * Num::int(187) >= time);
    assert!(rate * Num::int(186) < time);
    // Half way at a share of 0, 150 ticks of 0.5; at full power the other 75 take 75 ticks.
    let full = speed.site_rate(Num::ONE, time, power(5, 5));
    assert_eq!(full, Num::ONE);
    assert_eq!(Num::HALF * Num::int(150) + full * Num::int(75), time);
    // Two builders' rate of 1.5 slows alike: 0.75 a tick at a share of 0.
    let two = speed.site_rate(Num::int(3) / Num::int(2), time, none);
    assert_eq!(two, Num::int(3) / Num::int(4));
    // The slowest is `min`'s: 150 / 0.5 = 300, as at any share at most 0.5.
    assert_eq!(speed.slowest(Ticks::new(150)), Ticks::new(300));
    assert_eq!(of_150(speed, power(1, 100)), 300);
}
