use std::cell::RefCell;
use std::ops::ControlFlow;

use bevy_ecs::query::{Has, ROQueryItem};
use bevy_ecs::world::World;
use campfire_geometry::{BodyGrid, GridSearch, Indexed, Placed, Shape};
use campfire_math::{Num, Vec3};
use campfire_script::{AnyKeptRows, Checked, KeptRows, MaybeName, RowFill, Unit, View, ViewColumn};
use campfire_sim::StableId;
use campfire_units::{Body, Layer, MoveStep, WallBook};
use campfire_values::{PathId, Team};
use campfire_visibility::DataKind;

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
    standing: RefCell<StandingBodies>,
}

/// The bodies of the living units of the view that stand, by row, which a test of room indexes
/// once after each read, and the buffer it gathers them in.
#[derive(Debug, Default)]
struct StandingBodies {
    grid: BodyGrid<usize, StableId>,
    built: bool,
    gathered: Vec<Indexed<usize, StableId>>,
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

/// A search for room for a body of `shape` on `layer` on the points of `grid`, each a gap of the
/// grid clear of each other body and wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomSearch {
    pub grid: GridSearch,
    pub shape: Shape,
    pub layer: Layer,
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
        self.standing.get_mut().built = false;
        let walls = world.resource::<WallBook>();
        if !walls.same(&self.walls) {
            self.walls = walls.clone();
        }
        false
    }

    /// None: it reads no team's sight.
    fn hides(&self, _: usize, _: Team) -> bool {
        false
    }
}

impl StandingBodies {
    /// Indexes the bodies of the living units of `view` that stand, as `rows` say, unless it did
    /// since the read began.
    fn build(&mut self, view: &View, rows: &[NavigationRow]) {
        if self.built {
            return;
        }
        self.gathered.clear();
        view.each_row(|place, row| {
            if !rows[place].walks && row.alive && row.shape != Shape::POINT {
                self.gathered.push(Indexed {
                    id: row.id,
                    key: place,
                    body: Placed::new(row.position, row.shape),
                });
            }
        });
        self.grid.rebuild(self.gathered.drain(..));
        self.built = true;
    }
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
            if column.walls.overlaps(placed, layer) {
                return false;
            }
            let rows = column.rows.now();
            let mut standing = column.standing.borrow_mut();
            standing.build(view, rows);
            let mut clear = true;
            standing
                .grid
                .visit_near(position, placed.shape.bound(), |entry| {
                    if rows[entry.key].layer == layer && placed.overlaps(entry.body) {
                        clear = false;
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                });
            clear
        });
        room.unwrap_or(true)
    }

    /// The nearest point of `search`'s grid where its shape has room as `room_for` tests it, with
    /// the grid's gap, against `view` and `spawning`, of those that connect, room by room a gap
    /// apart, to the edge of the search, so no body the call spawned encloses it; of any room when
    /// none connects; none when no point has room (04-capabilities/navigation.md, A script finds
    /// room near a point; 04-capabilities/navigation.md, A room near a point connects).
    pub fn room_near(view: &View, search: RoomSearch, spawning: &[IndexedBody]) -> Option<Vec3> {
        let RoomSearch { grid, shape, layer } = search;
        let room = |position| {
            let ask = RoomAsk {
                position,
                shape,
                layer,
                gap: grid.gap,
            };
            Self::room_for(view, ask, spawning)
        };
        let reached = SearchFlood::of(grid, room);
        grid.nearest(|position| reached.holds(grid, position))
            .or_else(|| grid.nearest(room))
    }

    /// `unit.path`: the name of the path `unit` walks or stands on, `()` with none, or in a view
    /// with no navigation.
    pub(crate) fn path(unit: &Unit) -> Checked<MaybeName<PathId>> {
        let view = unit.view();
        let row = unit.admit(DataKind::Prediction)?;
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

/// The points of a search's grid, by their steps from its center, that have room and connect,
/// room by room along the grid's axes, to a point at the search's edge, one past which the next
/// step would leave it.
#[derive(Debug)]
struct SearchFlood {
    steps: i64,
    reached: Vec<bool>,
}

impl SearchFlood {
    /// The flood of `grid`'s points where `room` holds, from those at its edge.
    fn of(grid: GridSearch, room: impl Fn(Vec3) -> bool) -> Self {
        let steps = i64::from(grid.steps);
        let side = usize::try_from(2 * steps + 1).expect("a search's side fits usize");
        let most = steps * steps;
        let inside = |[i, j]: [i64; 2]| i * i + j * j <= most;
        let at = |[i, j]: [i64; 2]| {
            let step = |count: i64| {
                i64::try_from(i128::from(count) * i128::from(grid.gap.to_bits()))
                    .ok()
                    .map(Num::from_bits)
            };
            let offset = Vec3::new(step(i)?, Num::ZERO, step(j)?);
            grid.position
                .checked_add(offset)
                .filter(|at| at.within_bound())
        };
        let index = |[i, j]: [i64; 2]| {
            usize::try_from((i + steps) * (2 * steps + 1) + (j + steps)).expect("a step inside")
        };
        let mut open = vec![false; side * side];
        let mut reached = vec![false; side * side];
        let mut queue = Vec::new();
        for i in -steps..=steps {
            for j in -steps..=steps {
                let cell = [i, j];
                if !inside(cell) || !at(cell).is_some_and(&room) {
                    continue;
                }
                open[index(cell)] = true;
                let edge = [[1, 0], [-1, 0], [0, 1], [0, -1]]
                    .iter()
                    .any(|[di, dj]| !inside([i + di, j + dj]));
                if edge {
                    reached[index(cell)] = true;
                    queue.push(cell);
                }
            }
        }
        while let Some([i, j]) = queue.pop() {
            for [di, dj] in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
                let next = [i + di, j + dj];
                if inside(next) && open[index(next)] && !reached[index(next)] {
                    reached[index(next)] = true;
                    queue.push(next);
                }
            }
        }
        Self { steps, reached }
    }

    /// Whether `position`, a point of `grid`, the search it flooded, is reached.
    fn holds(&self, grid: GridSearch, position: Vec3) -> bool {
        let gap = i128::from(grid.gap.to_bits());
        let step = |at: Num, center: Num| {
            let bits = i128::from(at.to_bits()) - i128::from(center.to_bits());
            i64::try_from(bits / gap).expect("a point of the search")
        };
        let [i, j] = [
            step(position.x, grid.position.x),
            step(position.z, grid.position.z),
        ];
        if i.abs() > self.steps || j.abs() > self.steps {
            return false;
        }
        let side = 2 * self.steps + 1;
        let index = usize::try_from((i + self.steps) * side + (j + self.steps))
            .expect("a step within the search");
        self.reached[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flood_reaches_the_rooms_that_connect_to_the_searchs_edge() {
        // A search of 3 steps of a meter round the origin; room everywhere but a ring of the
        // square of 2 m, which encloses the 9 points within 1 m: those are not reached, as no step
        // along the axes leaves them, and the nearest reached point lies 3 m off, along +x
        // (04-capabilities/navigation.md, A room near a point connects).
        let grid = GridSearch {
            position: Vec3::ZERO,
            gap: Num::ONE,
            steps: 3,
        };
        let ring = |at: Vec3| {
            let [x, z] = [at.x, at.z].map(|axis| axis.to_int().unwrap().abs());
            x.max(z) == 2
        };
        let flood = SearchFlood::of(grid, |at| !ring(at));
        let point = |x: i64, z: i64| Vec3::new(Num::int(x), Num::ZERO, Num::int(z));
        assert!(!flood.holds(grid, point(0, 0)) && !flood.holds(grid, point(1, -1)));
        assert!(!flood.holds(grid, point(2, 0)));
        assert!(flood.holds(grid, point(3, 0)) && flood.holds(grid, point(0, -3)));
        assert_eq!(grid.nearest(|at| flood.holds(grid, at)), Some(point(3, 0)));
        // With no ring, the origin itself is reached.
        let open = SearchFlood::of(grid, |_| true);
        assert!(open.holds(grid, point(0, 0)));
    }
}
