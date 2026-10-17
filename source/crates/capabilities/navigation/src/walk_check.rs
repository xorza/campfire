use campfire_common::DeclaredName;
use campfire_geometry::Shape;
use campfire_math::{Num, Vec3};
use campfire_sim::IdAllocator;
use campfire_units::{Body, MapBook};
use campfire_values::UnitType;

use crate::body_index::{BodyIndex, IndexedBody};
use crate::error::MapProblem;
use crate::navigation_book::NavigationBook;
use crate::pathing_grid::PathingGrid;
use crate::route_planner::Walkable;
use crate::segment::Segment;
use crate::terrain::Terrain;
use crate::walker_kinds::WalkerKinds;

/// What a map's walk check reads: its ground, with the bounds; its paths, walls and pathing cells,
/// none for a map with no `[navigation]`; each kind of unit that walks; the bodies of its placed
/// units that do not walk; and the points of the markers its mode's units spawn at.
#[derive(Debug, Clone, Copy)]
pub struct WalkCheck<'a> {
    pub ground: &'a MapBook,
    pub navigation: &'a NavigationBook,
    pub walkers: &'a WalkerKinds,
    pub placed: &'a [PlacedBody],
    pub spawns: &'a [MarkerPoint],
}

/// A placed unit's body that does not walk: its place among the map's placed units, its type,
/// where it stands, and its body there.
#[derive(Debug, Clone, Copy)]
pub struct PlacedBody {
    pub unit: usize,
    pub unit_type: UnitType,
    pub pos: Vec3,
    pub body: Body,
}

/// A marker's point, by the marker's name.
#[derive(Debug, Clone)]
pub struct MarkerPoint {
    pub name: DeclaredName,
    pub pos: Vec3,
}

impl WalkCheck<'_> {
    /// Checks that each placed box lies within the bounds, and that the map can be walked by every
    /// kind of unit that walks, of the walkers, for the widest of each layer, among the placed
    /// bodies; `name_of` names a type in a problem. Placed bodies may overlap the walls and each
    /// other, as they block alike; a placement in a match refuses an overlap
    /// (04-capabilities/navigation.md, Placed obstacles may overlap). Every spawn point, and every
    /// waypoint, is a place that walker may stand, and every waypoint is in a reachable set of the
    /// one before it, by the regions a match plans its routes with. A narrower walker of the layer
    /// has every cell the widest has open. A map with no cells, or a mode with no walker, has
    /// nothing more to check.
    pub fn run(&self, name_of: impl Fn(UnitType) -> DeclaredName) -> Result<(), MapProblem> {
        let bounds = &self.ground.bounds;
        let NavigationBook {
            paths,
            walls: placed_walls,
            pathing,
        } = self.navigation;
        let mut ids = IdAllocator::default();
        let structures: Vec<IndexedBody> = self
            .placed
            .iter()
            .map(|placed| IndexedBody::of(ids.allocate(), placed.pos, &placed.body))
            .collect();
        for (placed, body) in self.placed.iter().zip(&structures) {
            let Shape::Box(boxed) = body.shape else {
                continue;
            };
            if !bounds.holds_box(body.pos, &boxed) {
                return Err(MapProblem::BoxOutside {
                    unit: placed.unit,
                    unit_type: name_of(placed.unit_type),
                });
            }
        }
        let &Some(cells) = pathing else {
            return Ok(());
        };
        let walkers = self.walkers;
        let terrain = Terrain::new(&cells, placed_walls.walls());
        let widest_radius = walkers.iter().map(|walker| walker.radius).max();
        let mut statics = BodyIndex::new(widest_radius.unwrap_or(Num::ZERO));
        statics.update(&structures);
        let widest: WalkerKinds = walkers.widest().collect();
        let mut grid = PathingGrid::new(cells, widest.clone(), &terrain);
        grid.update(&statics);
        let declared = |name: &str| DeclaredName::new(name).expect("the map's names are declared");
        for &walker in &widest {
            let clearance = grid.clearance(walker);
            let walkable = Walkable::of(clearance, &statics);
            let stands = |pos: Vec3| !walkable.blocks(Segment::new(pos, pos));
            if let Some(spawn) = self.spawns.iter().find(|spawn| !stands(spawn.pos)) {
                let marker = spawn.name.clone();
                return Err(MapProblem::MarkerBlocked { marker });
            }
            let reach = |pos: Vec3| clearance.regions().reach(cells.nearest_cell(pos));
            for (path, name) in paths.names().iter() {
                let points = paths.points(path);
                if let Some(waypoint) = points.iter().position(|&point| !stands(point)) {
                    let path = declared(name);
                    return Err(MapProblem::WaypointBlocked { path, waypoint });
                }
                let closed = points
                    .windows(2)
                    .position(|pair| !reach(pair[0]).meets(reach(pair[1])));
                if let Some(before) = closed {
                    let (path, waypoint) = (declared(name), before + 1);
                    return Err(MapProblem::WaypointUnreachable { path, waypoint });
                }
            }
        }
        Ok(())
    }
}
