use std::ops::Range;

use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_script::rhai::{Array, Dynamic};
use campfire_sim::Position;

use crate::scripts::error::Checked;
use crate::units::kept_rows::KeptRows;
use crate::units::relations::Relations;
use crate::units::row_fill::RowFill;
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::units::unit::Unit;
use crate::units::view_column::{ViewColumn, ViewColumns};
use crate::vision::seen_by::SeenBy;

/// The parts of a unit vision reads into its row: the teams that saw it, and its team.
pub(super) type RowParts = (Option<&'static SeenBy>, Option<&'static Team>);

/// What vision adds to the script view: the teams that see each unit, a row each.
#[derive(Debug, Default)]
pub(crate) struct VisionColumn {
    rows: KeptRows<Vec<TeamSet>>,
}

impl ViewColumn for VisionColumn {
    fn begin(&mut self, _: &World) -> bool {
        self.rows.begin();
        false
    }

    fn keep(&mut self, rows: Range<usize>) {
        self.rows.keep(rows);
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }

    fn same_as_kept(&self) -> bool {
        self.rows.same_as_kept()
    }
}

impl VisionColumn {
    /// Adds the row of a unit `seen_by` sees.
    pub(crate) fn push(&mut self, seen_by: TeamSet) {
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
    pub(crate) fn find(of: &Unit, pos: Position, radius: Num, filter: &str) -> Checked<Array> {
        let team = of.read(|row| row.team);
        of.view().find(of, pos, radius, filter, |columns, row| {
            VisionColumn::sees(columns, row, team)
        })
    }

    /// `ctx.nearest_visible`: the nearest living target `radius` from the edge of `of`'s body
    /// reaches that `filter` selects and `of`'s team sees.
    pub(crate) fn nearest(of: &Unit, radius: Num, filter: &str) -> Checked<Dynamic> {
        let team = of.read(|row| row.team);
        of.view().nearest(of, radius, filter, |columns, row| {
            VisionColumn::sees(columns, row, team)
        })
    }

    /// Fills a row of the script view with the teams that see the unit.
    pub(super) fn fill_row(
        parts: ROQueryItem<'_, '_, RowParts>,
        fill: &mut RowFill<'_, VisionColumn>,
    ) {
        let seen_by = Self::seen_by(parts, fill.relations);
        fill.column.push(seen_by);
    }

    /// The teams that see `unit`: those the last Vision stage found, or, before it ran, the
    /// unit's vision group under `relations`, as that stage would give it at the least; every
    /// team for an entity with no team, as a match without vision sees.
    pub(super) fn seen_by(parts: ROQueryItem<'_, '_, RowParts>, relations: &Relations) -> TeamSet {
        match parts {
            (Some(seen), _) => seen.get(),
            (None, Some(&team)) => relations.vision_group(team),
            (None, None) => TeamSet::ALL,
        }
    }
}
