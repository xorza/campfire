use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;
use campfire_sim::StableId;

use crate::units::kept_rows::KeptRows;
use crate::units::path_id::PathId;
use crate::units::script_view::View;
use crate::units::unit::Unit;
use crate::units::view_column::ViewColumn;

/// What navigation adds to the script view, a row each: the path each unit walks or stands on,
/// and whether it walks, as a unit with a step does.
#[derive(Debug, Default)]
pub(crate) struct NavigationColumn {
    rows: KeptRows<Vec<NavigationRow>>,
}

/// A unit's row of the navigation column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NavigationRow {
    path: Option<PathId>,
    walks: bool,
}

impl ViewColumn for NavigationColumn {
    fn begin(&mut self, _: &World) -> bool {
        self.rows.begin();
        false
    }

    fn keep(&mut self, row: usize) {
        self.rows.keep(row);
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }

    fn same_as_kept(&self) -> bool {
        self.rows.same_as_kept()
    }
}

impl NavigationColumn {
    /// Adds the row of a unit on `path`, which walks when `walks`.
    pub(crate) fn push(&mut self, path: Option<PathId>, walks: bool) {
        self.rows.now_mut().push(NavigationRow { path, walks });
    }

    /// `unit.path`: the name of the path `unit` walks or stands on, `()` with none, or in a view
    /// with no navigation.
    pub(crate) fn path(unit: &Unit) -> Dynamic {
        let view = unit.view();
        let path =
            view.column(|column: &NavigationColumn| column.rows.now()[unit.row_index()].path);
        view.path_name(path.flatten())
    }

    /// Whether unit `id`, which `view` read, walks; none does in a view with no navigation.
    pub(crate) fn walks(view: &View, id: StableId) -> bool {
        let Some(row) = view.row_index(id) else {
            return false;
        };
        let walks = view.column(|column: &NavigationColumn| column.rows.now()[row].walks);
        walks.unwrap_or(false)
    }
}
