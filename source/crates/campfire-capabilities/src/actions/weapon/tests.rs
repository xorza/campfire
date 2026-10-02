use std::str::FromStr;

use super::*;

fn weapon() -> Weapon {
    Weapon {
        rate: StatId::new(0),
        damage: StatId::new(1),
        kind: DamageKind::new(0),
    }
}

fn decimal(text: &str) -> Num {
    Num::from_str(text).unwrap()
}

#[test]
fn a_period_is_the_tick_rate_over_the_rate_rounded_up_and_past_the_windup() {
    // 0.67 attacks a second is 11240734.72 / 2²⁴, to 11240735: 30 × 2²⁴ over it is 44.78,
    // up to 45 ticks; 20 × 2²⁴ over it, 29.85, up to 30. Three a second is 10 ticks at 30
    // exactly, but must outlast a windup of 10. No rate, or one not positive, counts as one
    // bit: 30 × 2²⁴ ticks.
    let slow = [decimal("0.67")];
    let three = [Num::from_int(3).unwrap()];
    let none = [Num::from_int(-2).unwrap()];
    let cases = [
        (&slow[..], 30, 0, 45),
        (&slow[..], 20, 0, 30),
        (&three[..], 30, 9, 10),
        (&three[..], 30, 10, 11),
        (&none[..], 30, 0, 30 << 24),
        (&[][..], 30, 0, 30 << 24),
    ];
    for (values, hz, windup, period) in cases {
        let got = weapon().period(values, hz, Ticks::new(windup));
        assert_eq!(got, Ticks::new(period), "{values:?} at {hz} after {windup}");
    }
}

#[test]
fn damage_is_the_stat_at_its_place_and_never_negative() {
    let damage = |values: &[Num]| weapon().damage(values);
    let rate = Num::ONE;
    assert_eq!(damage(&[rate, decimal("23.5")]), decimal("23.5"));
    assert_eq!(damage(&[rate, decimal("-5")]), Num::ZERO);
    assert_eq!(damage(&[rate]), Num::ZERO);
}
