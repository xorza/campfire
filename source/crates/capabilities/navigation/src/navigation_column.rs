use std::cmp::Ordering;

use bevy_ecs::query::{Has, ROQueryItem};
use bevy_ecs::world::World;
use campfire_geometry::{Placed, Shape};
use campfire_math::{Num, Vec3};
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
/// which a body tests its room against.
#[derive(Debug, Default)]
pub struct NavigationColumn {
    rows: KeptRows<Vec<NavigationRow>>,
    walls: WallBook,
}

/// A body of `shape` on `layer` that would stand at `position`, and how far clear of each other
/// body and wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomAsk {
    pub position: Vec3,
    pub shape: Shape,
    pub layer: Layer,
    pub gap: Num,
}

/// A search for room for a body of `shape` on `layer` near `position`: on a grid of `gap` round
/// it, `gap` clear of each other body and wall, within `steps` gaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomSearch {
    pub position: Vec3,
    pub shape: Shape,
    pub layer: Layer,
    pub gap: Num,
    pub steps: u32,
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

impl RoomSearch {
    /// The most steps of its gap a search reaches: its points, about `π` times its square, stay
    /// within a mode call's work.
    pub const MAX_STEPS: u32 = 128;
}

impl NavigationColumn {
    /// Adds the row of a unit on `path`, which walks when `walks`, on `layer`.
    fn push(&mut self, path: Option<PathId>, walks: bool, layer: Layer) {
        self.rows
            .now_mut()
            .push(NavigationRow { path, walks, layer });
    }

    /// Whether `ask`'s shape has room at its position, as `view` read the match: within the
    /// bounds, and grown by its gap, clear of the walls of its layer, of the living units of its
    /// layer that stand, and of `spawning`, the bodies the call already spawns; with no
    /// navigation, of the bounds and `spawning` alone (04-capabilities/navigation.md, Every body
    /// is tested for room).
    pub fn room_for(view: &View, ask: RoomAsk, spawning: &[IndexedBody]) -> bool {
        let RoomAsk {
            position,
            shape,
            layer,
            gap,
        } = ask;
        if !view.bounds().holds(position, &shape) {
            return false;
        }
        let Some(grown) = shape.grown(gap) else {
            return false;
        };
        let placed = Placed::new(position, grown);
        let clear_of_spawning = spawning
            .iter()
            .filter(|other| other.layer == layer)
            .all(|other| !other.overlaps(placed));
        if !clear_of_spawning {
            return false;
        }
        let room = view.column(|column: &Self| {
            let rows = column.rows.now();
            let mut clear = !column.walls.overlaps(placed, layer);
            view.each_row(|place, row| {
                let stands = !rows[place].walks && rows[place].layer == layer;
                if clear && stands && row.alive && row.shape != Shape::POINT {
                    clear = !placed.overlaps(Placed::new(row.position, row.shape));
                }
            });
            clear
        });
        room.unwrap_or(true)
    }

    /// The nearest point within `search`'s steps of its position, of the grid of its gap round
    /// the position, where its shape has room as `room_for` tests it, with that gap, against
    /// `view` and `spawning`; by distance, then by angle from `+x`, as a facing turns, each
    /// compared exactly; none when no point has room (04-capabilities/navigation.md, A script
    /// finds room near a point). The grid points are visited in bands of square distance, each
    /// twice the last, so a search that ends near the position orders few.
    pub fn room_near(view: &View, search: RoomSearch, spawning: &[IndexedBody]) -> Option<Vec3> {
        let RoomSearch {
            position,
            shape,
            layer,
            gap,
            steps,
        } = search;
        debug_assert!(
            gap > Num::ZERO && steps <= RoomSearch::MAX_STEPS,
            "a search of a positive gap, within its steps"
        );
        let gap_bits = i128::from(gap.to_bits());
        let most = i64::from(steps) * i64::from(steps);
        let mut points = Vec::new();
        let mut low = 0;
        while low <= most {
            let high = (low * 2).max(64).min(most + 1);
            let reach = ((high - 1).cast_unsigned().isqrt()).cast_signed();
            points.clear();
            for i in -reach..=reach {
                for j in -reach..=reach {
                    if (low..high).contains(&(i * i + j * j)) {
                        points.push([i, j]);
                    }
                }
            }
            points.sort_unstable_by(|&a, &b| {
                let square = |[i, j]: [i64; 2]| i * i + j * j;
                square(a).cmp(&square(b)).then_with(|| Self::by_angle(a, b))
            });
            for &[i, j] in &points {
                let step = |count: i64| {
                    i64::try_from(i128::from(count) * gap_bits)
                        .ok()
                        .map(Num::from_bits)
                };
                let at = step(i)
                    .zip(step(j))
                    .and_then(|(x, z)| position.checked_add(Vec3::new(x, Num::ZERO, z)));
                let Some(at) = at.filter(|at| at.within_bound()) else {
                    continue;
                };
                let ask = RoomAsk {
                    position: at,
                    shape,
                    layer,
                    gap,
                };
                if Self::room_for(view, ask, spawning) {
                    return Some(at);
                }
            }
            low = high;
        }
        None
    }

    /// How two grid steps `[x, z]` of one length order by angle from `+x`, as a facing turns,
    /// toward `−z`: the half from `+x` through `−z`, then the other, each by the sign of the
    /// cross product.
    fn by_angle(a: [i64; 2], b: [i64; 2]) -> Ordering {
        let half = |[x, z]: [i64; 2]| u8::from(!(z < 0 || (z == 0 && x > 0)));
        half(a)
            .cmp(&half(b))
            .then_with(|| (a[1] * b[0] - a[0] * b[1]).cmp(&0).reverse())
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
