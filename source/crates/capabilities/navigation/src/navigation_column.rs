use bevy_ecs::query::{Has, ROQueryItem};
use bevy_ecs::world::World;
use campfire_geometry::Shape;
use campfire_math::Vec3;
use campfire_script::{AnyKeptRows, KeptRows, RowFill, ScriptValue, Unit, View, ViewColumn};
use campfire_sim::StableId;
use campfire_units::{Body, Layer, MoveStep};
use campfire_values::PathId;

use crate::body_index::IndexedBody;
use crate::navigation_book::NavigationBook;
use crate::on_path::OnPath;
use crate::walls::Walls;

/// The parts of a unit navigation reads into its row: the path it is on, whether it walks, and
/// its body, for its layer.
pub(crate) type RowReads = (
    Option<&'static OnPath>,
    Has<MoveStep>,
    Option<&'static Body>,
);

/// What navigation adds to the script view, a row each: the path each unit walks or stands on,
/// whether it walks, as a unit with a step does, and the layer it moves on; and the map's walls,
/// which a box tests its room against.
#[derive(Debug, Default)]
pub struct NavigationColumn {
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
    fn kept(&self) -> &dyn AnyKeptRows {
        &self.rows
    }

    fn kept_mut(&mut self) -> &mut dyn AnyKeptRows {
        &mut self.rows
    }

    fn begin(&mut self, world: &World) -> bool {
        let walls = &world.resource::<NavigationBook>().walls;
        if !walls.same(&self.walls) {
            self.walls = walls.clone();
        }
        false
    }
}

impl NavigationColumn {
    /// Adds the row of a unit on `path`, which walks when `walks`, on `layer`.
    fn push(&mut self, path: Option<PathId>, walks: bool, layer: Layer) {
        self.rows
            .now_mut()
            .push(NavigationRow { path, walks, layer });
    }

    /// Whether `body`, a box that would spawn at `pos`, has room there, as `view` read the match:
    /// within the bounds, clear of the walls of its layer, of the living units of its layer that
    /// stand, and of `spawning`, the boxes the call already spawns; with no navigation, of the
    /// bounds and `spawning` alone.
    pub fn room_for(view: &View, pos: Vec3, body: Body, spawning: &[IndexedBody]) -> bool {
        let Shape::Box(boxed) = body.shape() else {
            return true;
        };
        let layer = body.layer();
        let clear_of_spawning = spawning
            .iter()
            .filter(|other| other.layer == layer)
            .all(|other| !other.overlaps_box(pos, &boxed));
        let room = view.column(|column: &Self| {
            let rows = column.rows.now();
            let mut clear = view.bounds().holds_box(pos, &boxed)
                && !column.walls.overlaps_box(pos, &boxed, layer);
            view.each_row(|place, row| {
                let stands = !rows[place].walks && rows[place].layer == layer;
                if clear && stands && row.alive && row.shape != Shape::POINT {
                    let other = IndexedBody {
                        id: row.id,
                        pos: row.pos,
                        shape: row.shape,
                        layer,
                    };
                    clear = !other.overlaps_box(pos, &boxed);
                }
            });
            clear
        });
        let room = room.unwrap_or_else(|| view.bounds().holds_box(pos, &boxed));
        room && clear_of_spawning
    }

    /// `unit.path`: the name of the path `unit` walks or stands on, `()` with none, or in a view
    /// with no navigation.
    pub(crate) fn path(unit: &Unit) -> ScriptValue {
        let view = unit.view();
        let path = view.column(|column: &Self| column.rows.now()[unit.row_index()].path);
        let name = path.flatten().and_then(|path| view.names().name(path));
        name.map_or(ScriptValue::UNIT, ScriptValue::of)
    }

    /// Whether unit `id`, which `view` read, walks; none does in a view with no navigation.
    pub(crate) fn walks(view: &View, id: StableId) -> bool {
        let Some(row) = view.row_index(id) else {
            return false;
        };
        let walks = view.column(|column: &Self| column.rows.now()[row].walks);
        walks.unwrap_or(false)
    }

    /// Fills a row of the script view with the path the unit walks or stands on, whether it walks,
    /// and its layer.
    pub(super) fn fill_row(
        (path, walks, body): ROQueryItem<'_, '_, RowReads>,
        fill: &mut RowFill<'_, Self>,
    ) {
        let path = path.map(|path| path.get());
        fill.column.push(path, walks, Body::layer_of(body));
    }
}
