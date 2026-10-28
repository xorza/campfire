use campfire_declarations::{ById, PerId};
use campfire_math::{Num, Rounding};
use campfire_units::StatId;
use campfire_values::{ActionId, NameId};

use crate::action_book::ActionBook;
use crate::action_range::ActionRange;
use crate::action_slots::{ActionSlot, ActionSlots};
use crate::rank_values::RankValues;

/// A weapon's range bonus: the stats whose sum stretches its range, times 1 plus the sum, as
/// Zero Hour's `RANGE` weapon bonus does, which the capability of weapons books for each weapon of
/// one, so the scan and the chase, which know no weapon, read it (transports.md, A container's
/// modifier holds on its riders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeBonus(Box<[StatId]>);

impl FromIterator<StatId> for RangeBonus {
    fn from_iter<I: IntoIterator<Item = StatId>>(stats: I) -> Self {
        let stats: Box<[StatId]> = stats.into_iter().collect();
        debug_assert!(!stats.is_empty(), "a bonus of one stat at least");
        Self(stats)
    }
}

impl<const N: usize> From<[StatId; N]> for RangeBonus {
    fn from(stats: [StatId; N]) -> Self {
        stats.into_iter().collect()
    }
}

impl RangeBonus {
    /// `range` stretched by the bonus of a unit of stats `stats`: times 1 plus the sum of its
    /// stats' values, each 0 that the unit does not hold yet, rounded once to even, never below 0;
    /// a global range stays global.
    pub fn range(&self, range: ActionRange, stats: &[Num]) -> ActionRange {
        let ActionRange::Meters(meters) = range else {
            return range;
        };
        let bits = self.0.iter().fold(0_i128, |sum, stat| {
            let value = stats.get(stat.index()).copied().unwrap_or(Num::ZERO);
            sum + i128::from(value.to_bits())
        });
        let factor = Num::ONE
            .checked_add(Num::saturating_from_bits(bits))
            .unwrap_or(Num::MAX)
            .max(Num::ZERO);
        let stretched = meters.get().saturating_mul(factor, Rounding::NearestEven);
        ActionRange::meters(stretched)
    }

    /// `range` of the weapon `action` of a unit of stats `stats`, by the bonus of `bonuses`, when
    /// the weapon has one.
    pub fn of(
        bonuses: Option<&ById<ActionId, Self>>,
        action: ActionId,
        range: ActionRange,
        stats: &[Num],
    ) -> ActionRange {
        let bonus = bonuses.and_then(|bonuses| bonuses.get(action));
        bonus.map_or(range, |bonus| bonus.range(range, stats))
    }
}

/// What stretches a unit's weapons: the match's range bonuses, which a match without weapons of
/// one lacks, and the unit's stats.
#[derive(Debug, Clone, Copy)]
pub struct Stretch<'a> {
    pub bonuses: Option<&'a ById<ActionId, RangeBonus>>,
    pub stats: &'a [Num],
}

impl Stretch<'_> {
    /// The values of the action in `slot` of `slots`, as `book` gives them, its range stretched by
    /// its bonus.
    pub fn values_in(self, slots: &ActionSlots, book: &ActionBook, slot: u8) -> Option<RankValues> {
        let held = slots.slot(slot)?;
        self.values_of(held, book)
    }

    /// The values of the action of `slot`, as `book` gives them, its range stretched by its bonus.
    pub fn values_of(self, slot: ActionSlot, book: &ActionBook) -> Option<RankValues> {
        let mut values = *slot.values_in(book)?;
        let action = slot.action?;
        values.range = RangeBonus::of(self.bonuses, action, values.range, self.stats);
        Some(values)
    }
}

impl PerId<ActionId> for RangeBonus {
    const SCRIPTS_ONLY: bool = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bonus_stretches_a_range_by_one_plus_its_stats_sum() {
        let bonus = RangeBonus::from([StatId::new(0), StatId::new(2)]);
        let ten = ActionRange::meters(Num::int(10));
        // 0.33 and nothing for the stat it lacks: 10 × 1.33 = 13.3.
        let stats = [Num::from_bits(Num::ONE.to_bits() * 33 / 100), Num::int(5)];
        let stretched = bonus.range(ten, &stats);
        let expected = Num::int(10).saturating_mul(Num::ONE + stats[0], Rounding::NearestEven);
        assert_eq!(stretched, ActionRange::meters(expected));
        // None at all keeps the range, a global range stays, and a sum below −1 takes it to 0.
        assert_eq!(bonus.range(ten, &[]), ten);
        assert_eq!(
            bonus.range(ActionRange::Global, &stats),
            ActionRange::Global
        );
        let shrunk = bonus.range(ten, &[Num::int(-2)]);
        assert_eq!(shrunk, ActionRange::meters(Num::ZERO));
    }
}
