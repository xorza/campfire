use campfire_common::Ticks;
use campfire_math::{Num, Rounding};

use crate::stats::stat_id::StatId;
use crate::values::damage_kind::DamageKind;

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
        // A count of ticks, not a number: up to 2³² · 2²⁴ at the least rate, past a `Num`.
        let rate = i128::from(rate.to_bits().max(1));
        let ticks = Rounding::Ceiling.divide(i128::from(hz) << Num::FRAC_BITS, rate);
        let period = u64::try_from(ticks).expect("2⁵⁶ ticks at most");
        Ticks::new(period.max(windup.get() + 1))
    }

    /// The damage it deals for a unit of stats `values`, not negative.
    pub(crate) fn damage(self, values: &[Num]) -> Num {
        let damage = values.get(self.damage.index()).copied();
        damage.unwrap_or(Num::ZERO).max(Num::ZERO)
    }
}

#[cfg(test)]
mod tests;
