use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::query::Allow;
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, StableId, Unpredicted};
use campfire_values::UnitType;

use crate::level::Level;
use crate::param_book::ParamBook;
use crate::param_source::ParamSource;
use crate::stat_book::StatBook;
use crate::unit_stats::UnitStats;

/// What a system reads a modifier's params from: the param book, and the units a param scales by,
/// by stable id, those a predicting client holds as the server sent them among them
/// (10-replication-audiences.md, A client derives what it can for every unit it holds).
#[derive(SystemParam, Debug)]
pub struct ModifierParams<'w, 's> {
    params: Res<'w, ParamBook>,
    index: Res<'w, EntityIndex>,
    stats: Res<'w, StatBook>,
    units: Query<
        'w,
        's,
        (
            &'static UnitType,
            Option<&'static Level>,
            Option<&'static UnitStats>,
        ),
        Allow<Unpredicted>,
    >,
}

impl ModifierParams<'_, '_> {
    /// `id` as a source; `None` when it is gone.
    pub(crate) fn source(&self, id: StableId) -> Option<ParamSource<'_>> {
        let (&unit_type, level, stats) = self.units.get(self.index.get(id)?).ok()?;
        Some(ParamSource::of_parts(&self.stats, unit_type, level, stats))
    }

    pub(crate) fn book(&self) -> &ParamBook {
        &self.params
    }

    /// Whether the param book changed since the system last ran.
    pub fn is_changed(&self) -> bool {
        self.params.is_changed()
    }
}
