use std::str::FromStr;

use campfire_stats::PoolCost;

use super::*;
use crate::action_range::ActionRange;

fn weapon() -> Weapon {
    Weapon {
        rate: Source::Stat(StatId::new(0)),
        damage: Source::Stat(StatId::new(1)),
        kind: DamageKind::new(0),
    }
}

/// A weapon of its own period and amount.
const OWN: Weapon = Weapon {
    rate: Source::Own,
    damage: Source::Own,
    kind: DamageKind::new(0),
};

/// A rank's values of `windup` ticks, its own period `period` and amount `amount`.
fn values(windup: u64, period: Option<u64>, amount: Option<Num>) -> RankValues {
    RankValues {
        range: ActionRange::Global,
        cooldown: Ticks::ZERO,
        cost: PoolCost::default(),
        windup: Ticks::new(windup),
        charges: None,
        toggle: None,
        channel: None,
        charge: None,
        amount,
        period: period.map(Ticks::new),
        clip: None,
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
    // bit: 30 × 2²⁴ ticks; at the highest tick rate, (2³² − 1) × 2²⁴, past what a `Num` holds.
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
        (&none[..], u32::MAX, 0, u64::from(u32::MAX) << 24),
    ];
    for (values, hz, windup, period) in cases {
        let got = weapon().period(values, &self::values(windup, None, None), hz);
        assert_eq!(got, Ticks::new(period), "{values:?} at {hz} after {windup}");
    }
}

#[test]
fn damage_is_the_stat_at_its_place_and_never_negative() {
    let own = values(0, None, None);
    let damage = |stats: &[Num]| weapon().damage(stats, &own);
    let rate = Num::ONE;
    assert_eq!(damage(&[rate, decimal("23.5")]), decimal("23.5"));
    assert_eq!(damage(&[rate, decimal("-5")]), Num::ZERO);
    assert_eq!(damage(&[rate]), Num::ZERO);
}

#[test]
fn a_weapon_of_its_own_period_and_amount_reads_them_at_its_rank_and_no_stat() {
    // A period of 3 ticks after no windup is 3; after a windup of 3, it must outlast it: 4. Its
    // amount of 5 is dealt whatever its unit's stats hold.
    let stats = [Num::ONE, decimal("23.5")];
    assert_eq!(
        OWN.period(&stats, &values(0, Some(3), None), 30),
        Ticks::new(3)
    );
    assert_eq!(
        OWN.period(&stats, &values(3, Some(3), None), 30),
        Ticks::new(4)
    );
    let five = Num::from_int(5).unwrap();
    assert_eq!(OWN.damage(&stats, &values(0, None, Some(five))), five);
    assert_eq!(
        OWN.damage(&[], &values(0, None, Some(Num::ZERO))),
        Num::ZERO
    );
}
