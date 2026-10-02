use campfire_script::rhai::Dynamic;

use crate::units::path_id::PathId;
use crate::units::unit::Unit;
use crate::units::view_column::ViewColumn;

/// What navigation adds to the script view: the path each unit walks or stands on, a row each.
#[derive(Debug, Default)]
pub(crate) struct PathsColumn {
    rows: Vec<Option<PathId>>,
}

impl ViewColumn for PathsColumn {
    fn clear(&mut self) {
        self.rows.clear();
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }
}

impl PathsColumn {
    /// Adds the row of a unit on `path`.
    pub(crate) fn push(&mut self, path: Option<PathId>) {
        self.rows.push(path);
    }

    /// `unit.path`: the name of the path `unit` walks or stands on, `()` with none, or in a view
    /// with no navigation.
    pub(crate) fn path(unit: &Unit) -> Dynamic {
        let view = unit.view();
        let path = view.column(|column: &PathsColumn| column.rows[unit.row_index()]);
        view.path_name(path.flatten())
    }
}
