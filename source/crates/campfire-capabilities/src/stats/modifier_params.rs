use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, StableId};

use crate::stats::level::Level;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::param_book::ParamBook;
use crate::stats::param_read::ParamRead;
use crate::stats::param_source::ParamSource;
use crate::stats::stat_book::StatBook;
use crate::stats::unit_stats::UnitStats;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::units::unit_type::UnitType;
use crate::values::rank::Rank;

/// What a system reads a modifier's params from: the param book, and the units a param scales by,
/// by stable id.
#[derive(SystemParam, Debug)]
pub(crate) struct ModifierParams<'w, 's> {
    params: Res<'w, ParamBook>,
    index: Res<'w, EntityIndex>,
    stats: Option<Res<'w, StatBook>>,
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

impl ModifierParams<'_, '_> {
    /// `unit` as a source; `None` when it is gone, or the match has no stat book.
    pub(crate) fn source(&self, unit: StableId) -> Option<ParamSource<'_>> {
        let (&unit_type, level, stats) = self.units.get(self.index.get(unit)?).ok()?;
        let book = self.stats.as_deref()?;
        Some(ParamSource::of_parts(book, unit_type, level, stats))
    }

    /// The param at `place` of `modifier`, held by `ability` at `rank`, from `source`, as
    /// `ParamBook::modifier_param` reads it.
    pub(crate) fn modifier_param(
        &self,
        modifier: ModifierId,
        ability: Option<ActionId>,
        rank: Rank,
        place: &ParamPlace,
        source: Option<&ParamSource<'_>>,
    ) -> ParamRead {
        self.params
            .modifier_param(modifier, ability, rank, place, source)
    }

    /// Whether the param book changed since the system last ran.
    pub(crate) fn changed(&self) -> bool {
        self.params.is_changed()
    }
}
