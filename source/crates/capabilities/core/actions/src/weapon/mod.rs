use campfire_common::Ticks;
use campfire_math::{Num, Rounding};
use campfire_stats::StatId;
use campfire_values::{DamageKind, NameId};

use crate::rank_values::RankValues;

/// What makes an action of kind `attack` a weapon: where its rate and its damage come from, and
/// the kind of damage it deals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weapon {
    pub(crate) rate: Source,
    pub(crate) damage: Source,
    pub kind: DamageKind,
}

/// Where a weapon's value comes from: a place among its unit's stats, its rate in attacks a
/// second, or its own value at its rank, its period or its amount, which the load checks it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Stat(StatId),
    Own,
}

impl Weapon {
    /// The ticks from the start of one attack to the next at the earliest, at `hz` ticks a second,
    /// for a unit of stats `stats` and the weapon's values at its rank `values`: its own period,
    /// or the tick rate over its rate exactly, rounded up, at least one bit of a second; either
    /// longer than its windup.
    pub fn period(self, stats: &[Num], values: &RankValues, hz: u32) -> Ticks {
        let period = match self.rate {
            Source::Own => values
                .period
                .expect("a weapon of its own period has one at each rank"),
            Source::Stat(rate) => {
                let rate = stats.get(rate.index()).copied().unwrap_or(Num::ZERO);
                // A count of ticks, not a number: up to 2³² · 2²⁴ at the least rate, past a `Num`.
                let rate = i128::from(rate.to_bits().max(1));
                let ticks = Rounding::Ceiling.divide(i128::from(hz) << Num::FRAC_BITS, rate);
                Ticks::new(u64::try_from(ticks).expect("2⁵⁶ ticks at most"))
            }
        };
        Ticks::new(period.get().max(values.windup.get() + 1))
    }

    /// The damage it deals for a unit of stats `stats` with the weapon's values at its rank
    /// `values`: its own amount, or its stat, not negative.
    pub fn damage(self, stats: &[Num], values: &RankValues) -> Num {
        match self.damage {
            Source::Own => values
                .amount
                .expect("a weapon of its own damage has it at each rank"),
            Source::Stat(damage) => {
                let damage = stats.get(damage.index()).copied();
                damage.unwrap_or(Num::ZERO).max(Num::ZERO)
            }
        }
    }
}

#[cfg(test)]
mod tests;
