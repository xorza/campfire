use std::ops::Range;

use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_script::rhai::{Array, Dynamic};
use campfire_sim::Position;

use crate::scripts::error::Checked;
use crate::units::kept_rows::KeptRows;
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::units::unit::Unit;
use crate::units::view_column::{ViewColumn, ViewColumns};

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
        let seen = unit
            .view()
            .column(|column: &VisionColumn| column.rows.now()[other.row_index()].contains(team));
        seen.unwrap_or(true)
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
}
