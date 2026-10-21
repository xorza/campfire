use bevy_ecs::query::{Has, ROQueryItem};
use bevy_ecs::world::World;
use campfire_geometry::{Placed, Shape};
use campfire_math::Vec3;
use campfire_script::{AnyKeptRows, Checked, KeptRows, MaybeName, RowFill, Unit, View, ViewColumn};
use campfire_sim::StableId;
use campfire_units::{Body, Layer, MoveStep, WallBook};
use campfire_values::PathId;

use crate::body_index::IndexedBody;
use crate::on_path::OnPath;

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
    walls: WallBook,
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
        let walls = world.resource::<WallBook>();
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

    /// Whether `body`, a box that would spawn at `position`, has room there, as `view` read the
    /// match: within the bounds, clear of the walls of its layer, of the living units of its layer
    /// that stand, and of `spawning`, the boxes the call already spawns; with no navigation, of the
    /// bounds and `spawning` alone.
    pub fn room_for(view: &View, position: Vec3, body: Body, spawning: &[IndexedBody]) -> bool {
        let Shape::Box(boxed) = body.shape() else {
            return true;
        };
        let layer = body.layer();
        let clear_of_spawning = spawning
            .iter()
            .filter(|other| other.layer == layer)
            .all(|other| !other.overlaps_box(position, &boxed));
        let room = view.column(|column: &Self| {
            let rows = column.rows.now();
            let placed = Placed::new(position, Shape::Box(boxed));
            let mut clear = view.bounds().holds(position, &placed.shape)
                && !column.walls.overlaps(placed, layer);
            view.each_row(|place, row| {
                let stands = !rows[place].walks && rows[place].layer == layer;
                if clear && stands && row.alive && row.shape != Shape::POINT {
                    let other = IndexedBody {
                        id: row.id,
                        position: row.position,
                        shape: row.shape,
                        layer,
                    };
                    clear = !other.overlaps_box(position, &boxed);
                }
            });
            clear
        });
        let room = room.unwrap_or_else(|| view.bounds().holds(position, &Shape::Box(boxed)));
        room && clear_of_spawning
    }

    /// `unit.path`: the name of the path `unit` walks or stands on, `()` with none, or in a view
    /// with no navigation.
    pub(crate) fn path(unit: &Unit) -> Checked<MaybeName<PathId>> {
        let view = unit.view();
        let row = unit.row_index()?;
        let path = view.column(|column: &Self| column.rows.now()[row].path);
        Ok(MaybeName::from(
            path.flatten().map(|path| view.names().name(path)),
        ))
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
