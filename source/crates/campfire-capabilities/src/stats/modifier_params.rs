use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::query::Allow;
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, StableId, Unpredicted};

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
/// by stable id, those a predicting client holds as the server sent them among them (design 14,
/// D11).
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
        Allow<Unpredicted>,
    >,
}

impl ModifierParams<'_, '_> {
    /// `id` as a source; `None` when it is gone, or the match has no stat book.
    pub(crate) fn source(&self, id: StableId) -> Option<ParamSource<'_>> {
        let (&unit_type, level, stats) = self.units.get(self.index.get(id)?).ok()?;
        let book = self.stats.as_deref()?;
        Some(ParamSource::of_parts(book, unit_type, level, stats))
    }

    /// The param at `place` of `modifier`, held by `action` at `rank`, from `source`, as
    /// `ParamBook::modifier_param` reads it.
    pub(crate) fn modifier_param(
        &self,
        modifier: ModifierId,
        action: Option<ActionId>,
        rank: Rank,
        place: &ParamPlace,
        source: Option<&ParamSource<'_>>,
    ) -> ParamRead {
        self.params
            .modifier_param(modifier, action, rank, place, source)
    }

    /// Whether the param book changed since the system last ran.
    pub(crate) fn changed(&self) -> bool {
        self.params.is_changed()
    }
}
