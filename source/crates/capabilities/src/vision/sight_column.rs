use campfire_math::Num;
use campfire_script::rhai::{Array, Dynamic};
use campfire_sim::Position;

use crate::scripts::error::Checked;
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::units::unit::Unit;
use crate::units::view_column::ViewColumn;

/// What vision adds to the script view: the teams that see each unit, a row each.
#[derive(Debug, Default)]
pub(crate) struct SightColumn {
    rows: Vec<TeamSet>,
}

impl ViewColumn for SightColumn {
    fn clear(&mut self) {
        self.rows.clear();
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }
}

impl SightColumn {
    /// Adds the row of a unit `seen_by` sees.
    pub(crate) fn push(&mut self, seen_by: TeamSet) {
        self.rows.push(seen_by);
    }

    /// Whether `team` sees the unit of row `row` of `unit`'s view; every team does in a view with
    /// no vision.
    fn sees(unit: &Unit, row: usize, team: Team) -> bool {
        let seen = unit
            .view()
            .column(|column: &SightColumn| column.rows[row].contains(team));
        seen.unwrap_or(true)
    }

    /// `unit.can_see(other)`: whether `unit`'s team sees `other`.
    pub(crate) fn can_see(unit: &Unit, other: &Unit) -> bool {
        SightColumn::sees(unit, other.row_index(), unit.row().team)
    }

    /// `ctx.find_visible`: `ctx.find`, of the units `of`'s team sees.
    pub(crate) fn find(of: &Unit, pos: Position, radius: Num, filter: &str) -> Checked<Array> {
        let team = of.row().team;
        of.view().find(of, pos, radius, filter, |row| {
            SightColumn::sees(of, row, team)
        })
    }

    /// `ctx.nearest_visible`: the nearest living target `radius` from the edge of `of`'s body
    /// reaches that `filter` selects and `of`'s team sees.
    pub(crate) fn nearest(of: &Unit, radius: Num, filter: &str) -> Checked<Dynamic> {
        let team = of.row().team;
        of.view()
            .nearest(of, radius, filter, |row| SightColumn::sees(of, row, team))
    }
}
