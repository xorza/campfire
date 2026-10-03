use campfire_script::rhai::Dynamic;
use campfire_sim::StableId;

use crate::units::path_id::PathId;
use crate::units::script_view::View;
use crate::units::unit::Unit;
use crate::units::view_column::ViewColumn;

/// What navigation adds to the script view, a row each: the path each unit walks or stands on,
/// and whether it walks, as a unit with a step does.
#[derive(Debug, Default)]
pub(crate) struct NavigationColumn {
    paths: Vec<Option<PathId>>,
    walkers: Vec<bool>,
}

impl ViewColumn for NavigationColumn {
    fn clear(&mut self) {
        self.paths.clear();
        self.walkers.clear();
    }

    fn rows(&self) -> usize {
        self.paths.len()
    }
}

impl NavigationColumn {
    /// Adds the row of a unit on `path`, which walks when `walks`.
    pub(crate) fn push(&mut self, path: Option<PathId>, walks: bool) {
        self.paths.push(path);
        self.walkers.push(walks);
    }

    /// `unit.path`: the name of the path `unit` walks or stands on, `()` with none, or in a view
    /// with no navigation.
    pub(crate) fn path(unit: &Unit) -> Dynamic {
        let view = unit.view();
        let path = view.column(|column: &NavigationColumn| column.paths[unit.row_index()]);
        view.path_name(path.flatten())
    }

    /// Whether unit `id`, which `view` read, walks; none does in a view with no navigation.
    pub(crate) fn walks(view: &View, id: StableId) -> bool {
        let Some(row) = view.row_index(id) else {
            return false;
        };
        let walks = view.column(|column: &NavigationColumn| column.walkers[row]);
        walks.unwrap_or(false)
    }
}
