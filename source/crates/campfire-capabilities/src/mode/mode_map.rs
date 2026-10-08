use std::collections::BTreeMap;

use bevy_ecs::world::World;
use campfire_sim::Position;

use campfire_math::Num;
use campfire_sim::IdAllocator;

use crate::geometry::bounds::Bounds;
use crate::geometry::grid::Grid;
use crate::geometry::metric::Metric;
use crate::geometry::polygon::Polygon;
use crate::geometry::region::Region;
use crate::geometry::shape::Shape;
use crate::mode::error::ModeError;
use crate::mode::map_data::{MapData, MapPoint};
use crate::mode::mode_data::ModeParam;
use crate::mode::placed_unit::{PlacedPath, PlacedUnit};
use crate::mode::relation_data::RelationData;
use crate::mode::team_manifest::TeamManifest;
use crate::navigation::Navigation;
use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::error::MapProblem;
use crate::navigation::navigation_rules::NavigationRules;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::paths::Paths;
use crate::navigation::route_planner::Walkable;
use crate::navigation::segment::Segment;
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::navigation::wall::Wall;
use crate::navigation::walls::Walls;
use crate::units::body::BodyForm;
use crate::units::path_id::PathId;
use crate::units::relations::Relations;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::declared_name::DeclaredName;
use crate::values::name_list::NameList;
use crate::vision::vision_grid::VisionGrid;

/// The mode's map and the relations of its teams, every name resolved once, as the book builder
/// checks them: its ground, its paths, the units it places, its markers, and the cells vision
/// reveals, when it has them.
#[derive(Debug)]
pub struct ModeMap {
    pub(crate) ground: MapGround,
    pub(crate) paths: Paths,
    pub(crate) placed: Vec<PlacedUnit>,
    pub(crate) markers: Vec<MarkerSpec>,
    pub(crate) grid: Option<Grid>,
    /// The areas of the vision grid's brush, in the map's order.
    pub(crate) brush: Vec<Polygon>,
}

/// What a client's prediction takes of the map as a match does, and no script reads: its metric,
/// its bounds, how its teams regard each other, and the cells units plan routes over.
#[derive(Debug)]
pub(crate) struct MapGround {
    metric: Metric,
    bounds: Bounds,
    relations: Relations,
    pathing: Option<Grid>,
    /// The map's walls, which block cells on the pathing grid; none without it.
    walls: Vec<Wall>,
}

/// A marker of the map, names resolved: its name, its tags, its point, its region and its team
/// if it names them, and its params.
#[derive(Debug, Clone)]
pub(crate) struct MarkerSpec {
    pub(crate) name: Box<str>,
    pub(crate) tags: NameList,
    pub(crate) pos: Option<Position>,
    pub(crate) region: Option<Region>,
    pub(crate) team: Option<Team>,
    pub(crate) params: BTreeMap<DeclaredName, ModeParam>,
}

impl ModeMap {
    /// Whether `teams` name each team once, and are no more than a team index counts.
    fn check_teams(teams: &[TeamManifest]) -> Result<(), ModeError> {
        for (at, team) in teams.iter().enumerate() {
            if teams[..at].iter().any(|other| other.name == team.name) {
                return Err(ModeError::RepeatedName(team.name.clone()));
            }
        }
        if teams.len() > Team::LIMIT {
            return Err(ModeError::TooManyTeams);
        }
        Ok(())
    }

    /// The map `map` of a mode with `teams` and `relations`, its placed units' types resolved by
    /// `unit_type`, which knows the mode's types that stand. An error for what it names that the
    /// mode does not have: teams that share a name or more than `Team::LIMIT`, or more than
    /// `VisionGrid::MAX_TEAMS` with a vision grid; a relation of a team to itself, of a team the
    /// mode lacks, or of a pair named before; and in the map, grids that make no grid of its
    /// bounds, a wall on a layer of no name `rules` declares, a wall or a brush with points that
    /// make no simple polygon, a path with no waypoint or another's name, a placed unit of a type,
    /// team or path it lacks, or that walks from an end of no path, a marker of another's name, a
    /// team it lacks, or with a point and a region or a region outside the bounds, and any point
    /// that does not fit its metric or its bounds.
    pub(crate) fn resolve(
        map: &MapData,
        teams: &[TeamManifest],
        relations: &[RelationData],
        rules: &NavigationRules,
        unit_type: impl Fn(&str) -> Option<UnitType>,
    ) -> Result<ModeMap, ModeError> {
        ModeMap::check_teams(teams)?;
        let team = |name: &DeclaredName| {
            let at = teams.iter().position(|team| team.name == *name);
            let at = at.ok_or_else(|| ModeError::UnknownTeam(name.clone()))?;
            Ok(Team::new(u8::try_from(at).expect("teams fit their limit")))
        };
        let mut resolved = Relations::default();
        for (at, relation) in relations.iter().enumerate() {
            let [a, b] = &relation.teams;
            let pair = [team(a)?, team(b)?];
            let named = |data: &RelationData| {
                let [x, y] = &data.teams;
                (x == a && y == b) || (x == b && y == a)
            };
            if a == b || relations[..at].iter().any(named) {
                return Err(ModeError::RepeatedRelation(a.clone(), b.clone()));
            }
            resolved.set(pair[0], pair[1], relation.relation, relation.vision);
        }
        let grid = map.grid()?;
        let brush = map.brush()?;
        if grid.is_some() && teams.len() > VisionGrid::MAX_TEAMS {
            return Err(ModeError::TooManyVisionTeams);
        }
        let pathing = map.pathing()?;
        let walls = map.walls(rules)?;
        let point = |point: &MapPoint| map.point(point);
        let mut points = Vec::with_capacity(map.paths.len());
        for (at, path) in map.paths.iter().enumerate() {
            if map.paths[..at].iter().any(|other| other.name == path.name) {
                return Err(ModeError::RepeatedName(path.name.clone()));
            }
            if path.points.is_empty() {
                return Err(ModeError::EmptyPath(path.name.clone()));
            }
            let path_points = path.points.iter().map(point);
            points.push(path_points.collect::<Result<Vec<_>, _>>()?);
        }
        let names = map.paths.iter().map(|path| path.name.as_str());
        let paths = Paths::new(names.zip(points.iter().map(Vec::as_slice)));
        let mut placed = Vec::with_capacity(map.units.len());
        for unit in &map.units {
            let of_type = unit_type(unit.unit_type.as_str())
                .ok_or_else(|| ModeError::UnknownUnitType(unit.unit_type.clone()))?;
            let path = match (&unit.path, unit.from) {
                (Some(path), from) => {
                    let id = paths.named(path.as_str());
                    let path = id.ok_or_else(|| ModeError::UnknownPath(path.clone()))?;
                    Some(PlacedPath { path, from })
                }
                (None, Some(_)) => return Err(ModeError::NoPathToWalk(unit.unit_type.clone())),
                (None, None) => None,
            };
            placed.push(PlacedUnit {
                unit_type: of_type,
                team: team(&unit.team)?,
                path,
                pos: point(&unit.pos)?,
                angle: unit.angle,
            });
        }
        let markers = ModeMap::markers(map, team)?;
        Ok(ModeMap {
            ground: MapGround {
                metric: map.metric,
                bounds: map.bounds,
                relations: resolved,
                pathing,
                walls,
            },
            paths,
            placed,
            markers,
            grid,
            brush,
        })
    }

    /// The markers of `map`, each with its team as `team` resolves it; an error for two of one
    /// name, a team the mode lacks, or a region that is no box within the bounds or beside a
    /// point.
    fn markers(
        map: &MapData,
        team: impl Fn(&DeclaredName) -> Result<Team, ModeError>,
    ) -> Result<Vec<MarkerSpec>, ModeError> {
        let point = |point: &MapPoint| map.point(point);
        let mut markers = Vec::with_capacity(map.markers.len());
        for (at, marker) in map.markers.iter().enumerate() {
            if map.markers[..at]
                .iter()
                .any(|other| other.name == marker.name)
            {
                return Err(ModeError::RepeatedName(marker.name.clone()));
            }
            let marker_team = marker.team.as_ref().map(&team).transpose()?;
            let region = marker.region.map(|region| {
                region
                    .region(map.metric, map.bounds)
                    .filter(|_| marker.pos.is_none())
                    .ok_or_else(|| ModeError::Region(marker.name.clone()))
            });
            let region = region.transpose()?;
            markers.push(MarkerSpec {
                name: marker.name.as_str().into(),
                tags: marker.tags.iter().map(DeclaredName::as_str).collect(),
                pos: marker.pos.as_ref().map(point).transpose()?,
                region,
                team: marker_team,
                params: marker.params.clone(),
            });
        }
        Ok(markers)
    }

    /// Checks that each placed box has room, and that the map can be walked by every kind of
    /// unit that walks, of `walkers`, for the widest of each layer, among the map's placed units
    /// that cannot walk, whose bodies `body_of` gives by unit type, and none for a type that
    /// walks; `name_of` names a type in a problem. A box has room as a placement needs it: within
    /// the bounds, its inside clear of the walls of its layer and of every other placed unit of
    /// its layer that cannot walk. Every marker's point and waypoint is a place that walker may
    /// stand, and every waypoint is in a reachable set of the one before it, by the regions a
    /// match plans its routes with. A narrower walker of the layer has every cell the widest has
    /// open. A map with no `[navigation]` cells, or a mode with no walker, has nothing more to
    /// check.
    pub(crate) fn check_walkable(
        &self,
        walkers: &[Walker],
        body_of: impl Fn(UnitType) -> Option<BodyForm>,
        name_of: impl Fn(UnitType) -> DeclaredName,
    ) -> Result<(), MapProblem> {
        let MapGround {
            bounds,
            pathing,
            walls,
            ..
        } = &self.ground;
        let mut ids = IdAllocator::default();
        let structures: Vec<(usize, IndexedBody)> = self
            .placed
            .iter()
            .enumerate()
            .filter_map(|(at, unit)| {
                let body = body_of(unit.unit_type)?.at(unit.angle);
                Some((at, IndexedBody::of(ids.allocate(), unit.pos, &body)))
            })
            .collect();
        let placed_walls = Walls::new(walls);
        for &(at, body) in &structures {
            let Shape::Box(boxed) = body.shape else {
                continue;
            };
            let others = structures
                .iter()
                .filter(|(other, held)| *other != at && held.layer == body.layer)
                .map(|&(_, held)| held);
            if !placed_walls.room_for(*bounds, body.at, &boxed, body.layer, others) {
                return Err(MapProblem::BoxBlocked {
                    unit: at,
                    unit_type: name_of(self.placed[at].unit_type),
                });
            }
        }
        let structures: Vec<IndexedBody> = structures.into_iter().map(|(_, body)| body).collect();
        let Some(cells) = *pathing else {
            return Ok(());
        };
        let terrain = Terrain::new(&cells, walls);
        debug_assert!(walkers.is_sorted(), "walkers by layer, then radius");
        let widest = walkers
            .chunk_by(|a, b| a.layer == b.layer)
            .map(|layer| *layer.last().expect("a chunk is never empty"));
        let widest_radius = walkers.iter().map(|walker| walker.radius).max();
        let mut statics = BodyIndex::new(widest_radius.unwrap_or(Num::ZERO));
        statics.update(&structures);
        let mut grid = PathingGrid::new(cells, widest.clone().collect(), &terrain);
        grid.update(&statics);
        let declared = |name: &str| DeclaredName::new(name).expect("the map's names are declared");
        for walker in widest {
            let clearance = grid.clearance(walker);
            let walkable = Walkable {
                clearance,
                statics: &statics,
                short: None,
            };
            let stands = |at: Position| !walkable.blocks(Segment::new(at, at));
            for marker in &self.markers {
                if marker.pos.is_some_and(|pos| !stands(pos)) {
                    let marker = declared(&marker.name);
                    return Err(MapProblem::MarkerBlocked { marker });
                }
            }
            let reach = |at: Position| clearance.regions().reach(cells.nearest_cell(at));
            for (path, name) in self.paths.names().enumerate() {
                let points = self.paths.points(PathId::new(path));
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

impl MapGround {
    /// Puts the ground in `world`, its pathing grid for the kinds of `walkers`.
    pub(crate) fn install(self, world: &mut World, walkers: Vec<Walker>) {
        let MapGround {
            metric,
            bounds,
            relations,
            pathing,
            walls,
        } = self;
        world.insert_resource(metric);
        world.insert_resource(bounds);
        world.insert_resource(relations);
        if let Some(pathing) = pathing {
            Navigation::load_pathing(world, pathing, &walls, walkers);
        }
    }
}
