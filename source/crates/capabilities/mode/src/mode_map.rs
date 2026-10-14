use bevy_ecs::world::World;
use campfire_geometry::{Grid, Shape};
use campfire_math::Num;
use campfire_navigation::{
    BodyIndex, IndexedBody, MapProblem, Navigation, NavigationBook, PathingGrid, Paths, Segment,
    Terrain, Walkable, Walker, Walls,
};
use campfire_sim::{IdAllocator, Position};
use campfire_units::{BodyForm, Layer, MapBook};
use campfire_values::{DeclaredName, Names, NamesError, Relations, Team, UnitType};
use campfire_vision::{BrushMap, VisionBook, VisionGrid};

use crate::error::ModeError;
use crate::map_data::MapData;
use crate::map_point::MapPoint;
use crate::marker_spec::MarkerSpec;
use crate::placed_unit::{PlacedPath, PlacedUnit};
use crate::relation_data::RelationData;
use crate::team_manifest::TeamManifest;

/// The mode's map and the relations of its teams, every name resolved once, as the book builder
/// checks them, but what its books hold: the relations the match starts with, the units it
/// places, its markers, and the cells units plan routes over, when it has them.
#[derive(Debug)]
pub struct ModeMap {
    /// The mode's teams' names, by team.
    pub teams: Names<Team>,
    pub relations: Relations,
    pub placed: Vec<PlacedUnit>,
    pub markers: Vec<MarkerSpec>,
    pub pathing: Option<Grid>,
}

/// The mode's map as the book builder resolves it: the map a match starts from, and the books of
/// its ground, of its paths and walls, and of its vision grid.
#[derive(Debug)]
pub struct ResolvedMap {
    pub map: ModeMap,
    pub ground: MapBook,
    pub navigation: NavigationBook,
    pub vision: VisionBook,
}

impl ModeMap {
    /// The names of `teams`; an error for two of one name, or more than a team index counts.
    fn team_names(teams: &[TeamManifest]) -> Result<Names<Team>, ModeError> {
        let mut names = Names::default();
        for team in teams {
            match names.try_add(team.name.as_str()) {
                Ok(_) => {}
                Err(NamesError::Repeated) => {
                    return Err(ModeError::RepeatedName(team.name.clone()));
                }
                Err(NamesError::Full) => return Err(ModeError::TooManyTeams),
            }
        }
        Ok(names)
    }

    /// The map `map` of a mode with `teams` and `relations`, its placed units' types resolved by
    /// `unit_type`, which knows the mode's types that stand. An error for what it names that the
    /// mode does not have: teams that share a name or more than `Team::LIMIT`, or more than
    /// `VisionGrid::MAX_TEAMS` with a vision grid; a relation of a team to itself, of a team the
    /// mode lacks, or of a pair named before; and in the map, grids that make no grid of its
    /// bounds, a wall on a layer of no name `layers` holds, a wall or a brush with points that
    /// make no simple polygon, a path with no waypoint or another's name, a placed unit of a type,
    /// team or path it lacks, or that walks from an end of no path, a marker of another's name, a
    /// team it lacks, or with a point and a region or a region outside the bounds, and any point
    /// that does not fit its metric or its bounds.
    pub fn resolve(
        map: &MapData,
        teams: &[TeamManifest],
        relations: &[RelationData],
        layers: &Names<Layer>,
        unit_type: impl Fn(&str) -> Option<UnitType>,
    ) -> Result<ResolvedMap, ModeError> {
        let team_names = ModeMap::team_names(teams)?;
        let team = |name: &DeclaredName| {
            let team = team_names.named(name.as_str());
            team.ok_or_else(|| ModeError::UnknownTeam(name.clone()))
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
        let walls = map.walls(layers)?;
        let point = |point: &MapPoint| map.point(point);
        let mut path_names = Names::default();
        let mut points = Vec::with_capacity(map.paths.len());
        for path in &map.paths {
            if path_names.try_add(path.name.as_str()).is_err() {
                return Err(ModeError::RepeatedName(path.name.clone()));
            }
            if path.points.is_empty() {
                return Err(ModeError::EmptyPath(path.name.clone()));
            }
            let path_points = path.points.iter().map(point);
            points.push(path_points.collect::<Result<Vec<_>, _>>()?);
        }
        let paths = Paths::of(path_names, &points);
        let mut placed = Vec::with_capacity(map.units.len());
        for unit in &map.units {
            let of_type = unit_type(unit.unit_type.as_str())
                .ok_or_else(|| ModeError::UnknownUnitType(unit.unit_type.clone()))?;
            let path = match (&unit.path, unit.from) {
                (Some(path), from) => {
                    let id = paths.names().named(path.as_str());
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
                pos: map.placed_at(&unit.pos, unit.height)?,
                angle: unit.angle,
            });
        }
        let markers = ModeMap::markers(map, team)?;
        let vision = grid.map(|grid| VisionGrid {
            brush: BrushMap::new(&grid, &brush),
            grid,
            teams: team_names.len(),
        });
        Ok(ResolvedMap {
            map: ModeMap {
                teams: team_names,
                relations: resolved,
                placed,
                markers,
                pathing,
            },
            ground: MapBook {
                metric: map.metric,
                bounds: map.bounds,
            },
            navigation: NavigationBook {
                paths,
                walls: Walls::new(&walls),
            },
            vision: VisionBook(vision),
        })
    }

    /// Gives `world` the map's pathing grid, when it has one, for the kinds of `walkers`, among the
    /// walls of its navigation book.
    pub fn install_pathing(&self, world: &mut World, walkers: Vec<Walker>) {
        if let Some(grid) = self.pathing {
            Navigation::load_pathing(world, grid, walkers);
        }
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
                tags: marker.tags.iter().cloned().collect(),
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
    pub fn check_walkable(
        &self,
        ground: &MapBook,
        navigation: &NavigationBook,
        walkers: &[Walker],
        body_of: impl Fn(UnitType) -> Option<BodyForm>,
        name_of: impl Fn(UnitType) -> DeclaredName,
    ) -> Result<(), MapProblem> {
        let bounds = &ground.bounds;
        let NavigationBook {
            paths,
            walls: placed_walls,
        } = navigation;
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
        for &(at, body) in &structures {
            let Shape::Box(boxed) = body.shape else {
                continue;
            };
            let mut others = structures
                .iter()
                .filter(|(other, held)| *other != at && held.layer == body.layer);
            let walls = Some(placed_walls);
            let clear = Walls::room_for(walls, *bounds, body.pos, &boxed, body.layer)
                && !others.any(|(_, other)| other.overlaps_box(body.pos, &boxed));
            if !clear {
                return Err(MapProblem::BoxBlocked {
                    unit: at,
                    unit_type: name_of(self.placed[at].unit_type),
                });
            }
        }
        let structures: Vec<IndexedBody> = structures.into_iter().map(|(_, body)| body).collect();
        let Some(cells) = self.pathing else {
            return Ok(());
        };
        let terrain = Terrain::new(&cells, placed_walls.walls());
        debug_assert!(walkers.is_sorted(), "walkers by layer, then radius");
        let widest = walkers
            .chunk_by(|a, b| a.layer == b.layer)
            .map(|layer| *layer.last().expect("a chunk is never empty"));
        let widest_radius = walkers.iter().map(|walker| walker.radius).max();
        let mut statics = BodyIndex::new(widest_radius.unwrap_or(Num::ZERO));
        statics.update(&structures);
        let mut grid = PathingGrid::new(cells, widest.clone(), &terrain);
        grid.update(&statics);
        let declared = |name: &str| DeclaredName::new(name).expect("the map's names are declared");
        for walker in widest {
            let clearance = grid.clearance(walker);
            let walkable = Walkable::of(clearance, &statics);
            let stands = |pos: Position| !walkable.blocks(Segment::new(pos, pos));
            for marker in &self.markers {
                if marker.pos.is_some_and(|pos| !stands(pos)) {
                    let marker = declared(&marker.name);
                    return Err(MapProblem::MarkerBlocked { marker });
                }
            }
            let reach = |pos: Position| clearance.regions().reach(cells.nearest_cell(pos));
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
