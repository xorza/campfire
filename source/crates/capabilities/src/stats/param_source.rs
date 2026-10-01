use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{EntityIndex, StableId};

use crate::stats::level::Level;
use crate::stats::stat_book::StatBook;
use crate::stats::unit_stats::UnitStats;
use crate::units::unit_type::UnitType;

/// What a scaling param reads of its source: the source's level, its stats, and its type's
/// values at that level.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParamSource<'a> {
    book: &'a StatBook,
    unit_type: UnitType,
    level: u32,
    values: &'a [Num],
}

impl<'a> ParamSource<'a> {
    /// The source of `book`'s stats `values`, of `unit_type` at `level`; a stat past `values`, as
    /// of a unit whose stats are not yet derived, reads 0.
    pub(crate) const fn new(
        book: &'a StatBook,
        unit_type: UnitType,
        level: u32,
        values: &'a [Num],
    ) -> ParamSource<'a> {
        ParamSource {
            book,
            unit_type,
            level,
            values,
        }
    }

    /// `unit` in `world` as a source; `None` when it is gone, or the match has no stat book.
    pub(crate) fn of(world: &'a World, unit: StableId) -> Option<ParamSource<'a>> {
        let book = world.get_resource::<StatBook>()?;
        let entity = world.resource::<EntityIndex>().get(unit)?;
        let unit = world.entity(entity);
        let level = unit.get::<Level>().map_or(1, |level| level.get());
        let values = unit.get::<UnitStats>().map_or(&[][..], UnitStats::values);
        Some(ParamSource::new(
            book,
            *unit.get::<UnitType>()?,
            level,
            values,
        ))
    }

    pub(crate) const fn level(&self) -> u32 {
        self.level
    }

    /// Its stat at `at`, in bits.
    pub(crate) fn stat_bits(&self, at: u16) -> i128 {
        self.values
            .get(usize::from(at))
            .map_or(0, |value| i128::from(value.to_bits()))
    }

    /// The part of its stat at `at` above its type's value at its level, in bits.
    pub(crate) fn bonus_bits(&self, at: u16) -> i128 {
        self.stat_bits(at) - self.book.base_bits(self.unit_type, at, self.level)
    }
}
