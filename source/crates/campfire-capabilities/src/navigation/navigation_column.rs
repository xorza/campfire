use std::ops::Range;

use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;
use campfire_sim::{Position, StableId};

use crate::geometry::shape::Shape;
use crate::navigation::body_index::IndexedBody;
use crate::navigation::walls::Walls;
use crate::units::body::Body;
use crate::units::kept_rows::KeptRows;
use crate::units::layer::Layer;
use crate::units::path_id::PathId;
use crate::units::script_view::View;
use crate::units::unit::Unit;
use crate::units::view_column::ViewColumn;

/// What navigation adds to the script view, a row each: the path each unit walks or stands on,
/// whether it walks, as a unit with a step does, and the layer it moves on; and the map's walls,
/// which a box tests its room against.
#[derive(Debug, Default)]
pub(crate) struct NavigationColumn {
    rows: KeptRows<Vec<NavigationRow>>,
    walls: Walls,
}

/// A unit's row of the navigation column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NavigationRow {
    path: Option<PathId>,
    walks: bool,
    layer: Layer,
}

impl ViewColumn for NavigationColumn {
    fn begin(&mut self, world: &World) -> bool {
        self.rows.begin();
        if let Some(walls) = world.get_resource::<Walls>()
            && !walls.same(&self.walls)
        {
            self.walls = walls.clone();
        }
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

impl NavigationColumn {
    /// Adds the row of a unit on `path`, which walks when `walks`, on `layer`.
    pub(crate) fn push(&mut self, path: Option<PathId>, walks: bool, layer: Layer) {
        self.rows
            .now_mut()
            .push(NavigationRow { path, walks, layer });
    }

    /// Whether `body`, a box that would spawn at `at`, has room there, as `view` read the match:
    /// within the bounds, clear of the walls of its layer, of the living units of its layer that
    /// stand, and of `spawning`, the boxes the call already spawns; with no navigation, of the
    /// bounds and `spawning` alone.
    pub(crate) fn room_for(
        view: &View,
        at: Position,
        body: Body,
        spawning: &[IndexedBody],
    ) -> bool {
        let Shape::Box(boxed) = body.shape() else {
            return true;
        };
        let layer = body.layer();
        let clear_of_spawning = spawning
            .iter()
            .filter(|other| other.layer == layer)
            .all(|other| !other.overlaps_box(at, &boxed));
        let room = view.column(|column: &NavigationColumn| {
            let rows = column.rows.now();
            let mut clear = Walls::room_for(Some(&column.walls), view.bounds(), at, &boxed, layer);
            view.each_row(|place, row| {
                let stands = !rows[place].walks && rows[place].layer == layer;
                if clear && stands && row.alive && row.shape != Shape::POINT {
                    let other = IndexedBody {
                        id: row.id,
                        at: row.pos,
                        shape: row.shape,
                        layer,
                    };
                    clear = !other.overlaps_box(at, &boxed);
                }
            });
            clear
        });
        let room = room.unwrap_or_else(|| Walls::room_for(None, view.bounds(), at, &boxed, layer));
        room && clear_of_spawning
    }

    /// `unit.path`: the name of the path `unit` walks or stands on, `()` with none, or in a view
    /// with no navigation.
    pub(crate) fn path(unit: &Unit) -> Dynamic {
        let view = unit.view();
        let path =
            view.column(|column: &NavigationColumn| column.rows.now()[unit.row_index()].path);
        let name = path.flatten().and_then(|path| view.path_name(path));
        name.map_or(Dynamic::UNIT, Dynamic::from)
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
