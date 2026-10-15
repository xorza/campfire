use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_script::{
    AnyKeptRows, Filter, KeptRows, RowFill, ScriptList, ScriptRadius, ScriptValue, Unit,
    ViewColumn, ViewColumns,
};
use campfire_sim::Position;
use campfire_values::{Relations, Team, TeamSet};

use crate::seen_by::SeenBy;

/// The parts of a unit vision reads into its row: the teams that saw it, and its team.
pub(crate) type RowReads = (Option<&'static SeenBy>, Option<&'static Team>);

/// What vision adds to the script view: the teams that see each unit, a row each.
#[derive(Debug, Default)]
pub struct VisionColumn {
    rows: KeptRows<Vec<TeamSet>>,
}

impl ViewColumn for VisionColumn {
    fn kept(&self) -> &dyn AnyKeptRows {
        &self.rows
    }

    fn kept_mut(&mut self) -> &mut dyn AnyKeptRows {
        &mut self.rows
    }

    fn begin(&mut self, _: &World) -> bool {
        false
    }
}

impl VisionColumn {
    /// Adds the row of a unit `seen_by` sees.
    pub fn push(&mut self, seen_by: TeamSet) {
        self.rows.now_mut().push(seen_by);
    }

    /// Whether `team` sees the unit of row `row` of the view of `columns`; every team does in a
    /// view with no vision.
    fn sees(columns: &ViewColumns, row: usize, team: Team) -> bool {
        let column = columns.get::<VisionColumn>();
        column.is_none_or(|column| column.rows.now()[row].contains(team))
    }

    /// `unit.can_see(other)`: whether `unit`'s team sees `other`.
    pub(crate) fn can_see(unit: &Unit, other: &Unit) -> bool {
        let team = unit.read(|row| row.team);
        VisionColumn::sees(unit.view().rows().columns(), other.row_index(), team)
    }

    /// `ctx.find_visible`: `ctx.find`, of the units `of`'s team sees.
    pub(crate) fn find(
        of: &Unit,
        pos: Position,
        radius: ScriptRadius,
        filter: &Filter,
    ) -> ScriptList {
        let team = of.read(|row| row.team);
        of.view()
            .find(of, pos, radius.get(), filter, |columns, row| {
                VisionColumn::sees(columns, row, team)
            })
    }

    /// `ctx.nearest_visible`: the nearest living target `radius` from the edge of `of`'s body
    /// reaches that `filter` selects and `of`'s team sees.
    pub(crate) fn nearest(of: &Unit, radius: ScriptRadius, filter: &Filter) -> ScriptValue {
        let team = of.read(|row| row.team);
        of.view().nearest(of, radius.get(), filter, |columns, row| {
            VisionColumn::sees(columns, row, team)
        })
    }

    /// Fills a row of the script view with the teams that see the unit.
    pub(super) fn fill_row(
        parts: ROQueryItem<'_, '_, RowReads>,
        fill: &mut RowFill<'_, VisionColumn>,
    ) {
        let seen_by = Self::seen_by(parts, fill.relations);
        fill.column.push(seen_by);
    }

    /// The teams that see `unit`: those the last Vision stage found, or, before it ran, the
    /// unit's vision group under `relations`, as that stage would give it at the least; every
    /// team for an entity with no team, as a match without vision sees.
    pub(super) fn seen_by(parts: ROQueryItem<'_, '_, RowReads>, relations: &Relations) -> TeamSet {
        match (parts.0.and_then(|seen| seen.get()), parts.1) {
            (Some(teams), _) => teams,
            (None, Some(&team)) => relations.vision_group(team),
            (None, None) => TeamSet::ALL,
        }
    }
}
