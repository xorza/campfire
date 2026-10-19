use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{SimWorld, StableId};
use campfire_units::StatId;
use campfire_values::{NameId, UnitType};

use crate::level::Level;
use crate::stat_book::StatBook;
use crate::unit_stats::UnitStats;

/// What a scaling param reads of its source: the source's level, its stats, and its type's
/// values at that level.
#[derive(Debug, Clone, Copy)]
pub struct ParamSource<'a> {
    book: &'a StatBook,
    unit_type: UnitType,
    level: u32,
    values: &'a [Num],
}

impl<'a> ParamSource<'a> {
    /// The source of `book`'s stats `values`, of `unit_type` at `level`; a stat past `values`, as
    /// of a unit whose stats are not yet derived, reads 0.
    pub const fn new(
        book: &'a StatBook,
        unit_type: UnitType,
        level: u32,
        values: &'a [Num],
    ) -> Self {
        ParamSource {
            book,
            unit_type,
            level,
            values,
        }
    }

    /// A unit of `unit_type` as a source, of its `level`, 1 with none, and its `stats`, each 0
    /// before they are derived: how every reader of a unit's parts sees it as a source.
    pub(crate) fn of_parts(
        book: &'a StatBook,
        unit_type: UnitType,
        level: Option<&Level>,
        stats: Option<&'a UnitStats>,
    ) -> Self {
        let level = level.map_or(1, |level| level.get());
        let values = stats.map_or(&[][..], UnitStats::values);
        ParamSource::new(book, unit_type, level, values)
    }

    pub(crate) const fn level(&self) -> u32 {
        self.level
    }

    /// Its stat at `at`, in bits.
    pub(crate) fn stat_bits(&self, at: StatId) -> i128 {
        self.values
            .get(at.index())
            .map_or(0, |value| i128::from(value.to_bits()))
    }

    /// The part of its stat at `at` above its type's value at its level, in bits.
    pub(crate) fn bonus_bits(&self, at: StatId) -> i128 {
        self.stat_bits(at) - self.book.base_bits(self.unit_type, at, self.level)
    }

    /// `id` as a source; `None` when it is gone, or the match has no stat book.
    pub fn of(world: &World, id: StableId) -> Option<ParamSource<'_>> {
        let book = world.get_resource::<StatBook>()?;
        let entity = world.entity_of(id)?;
        let id = world.entity(entity);
        Some(ParamSource::of_parts(
            book,
            *id.get::<UnitType>()?,
            id.get::<Level>(),
            id.get::<UnitStats>(),
        ))
    }
}
