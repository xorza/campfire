use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, StableId};

use crate::stats::level::Level;
use crate::stats::param_source::ParamSource;
use crate::stats::stat_book::StatBook;
use crate::stats::unit_stats::UnitStats;
use crate::units::unit_type::UnitType;

/// The units a system reads as param sources, by stable id.
#[derive(SystemParam, Debug)]
pub(crate) struct ParamSources<'w, 's> {
    index: Res<'w, EntityIndex>,
    book: Option<Res<'w, StatBook>>,
    units: Query<
        'w,
        's,
        (
            &'static UnitType,
            Option<&'static Level>,
            Option<&'static UnitStats>,
        ),
    >,
}

impl ParamSources<'_, '_> {
    /// `unit` as a source; `None` when it is gone, or the match has no stat book.
    pub(crate) fn get(&self, unit: StableId) -> Option<ParamSource<'_>> {
        let (&unit_type, level, stats) = self.units.get(self.index.get(unit)?).ok()?;
        let book = self.book.as_deref()?;
        Some(ParamSource::of_parts(book, unit_type, level, stats))
    }
}
