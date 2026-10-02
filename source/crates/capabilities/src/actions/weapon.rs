use campfire_math::{Num, Ticks};

use crate::combat::damage_kind::DamageKind;
use crate::stats::stat_id::StatId;

/// What makes an action of kind `attack` a weapon: the places among its unit's stats of its rate,
/// in attacks a second, and of its damage, and the kind of damage it deals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Weapon {
    pub(crate) rate: StatId,
    pub(crate) damage: StatId,
    pub(crate) kind: DamageKind,
}

impl Weapon {
    /// The ticks from the start of one attack to the next at the earliest, at `hz` ticks a second
    /// for a unit of stats `values`: the tick rate over its rate exactly, rounded up, at least
    /// one bit of a second, and longer than `windup`.
    pub(crate) fn period(self, values: &[Num], hz: u32, windup: Ticks) -> Ticks {
        let rate = values.get(self.rate.index()).copied().unwrap_or(Num::ZERO);
        let attacks = (i128::from(rate.to_bits()) << Num::FRAC_BITS).max(1 << Num::FRAC_BITS);
        let period = (u128::from(hz) << (2 * Num::FRAC_BITS)).div_ceil(attacks.cast_unsigned());
        let period = u64::try_from(period).unwrap_or(u64::MAX);
        Ticks::new(period.max(windup.get() + 1))
    }

    /// The damage it deals for a unit of stats `values`, not negative.
    pub(crate) fn damage(self, values: &[Num]) -> Num {
        let damage = values.get(self.damage.index()).copied();
        damage.unwrap_or(Num::ZERO).max(Num::ZERO)
    }
}

#[cfg(test)]
mod tests {
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
}
