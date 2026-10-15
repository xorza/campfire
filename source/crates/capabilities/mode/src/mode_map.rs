use bevy_ecs::world::World;
use campfire_geometry::Grid;
use campfire_navigation::{
    MarkerPoint, Navigation, NavigationBook, Paths, PlacedBody, Walker, Walls,
};
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
    pub fn team_names(teams: &[TeamManifest]) -> Result<Names<Team>, ModeError> {
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

    /// The bodies of its placed units that do not walk, whose bodies `body_of` gives by unit
    /// type, and none for a type that walks: what a walk check takes of them.
    pub fn placed_bodies(&self, body_of: impl Fn(UnitType) -> Option<BodyForm>) -> Vec<PlacedBody> {
        let placed = self.placed.iter().enumerate();
        let bodies = placed.filter_map(|(unit, placed)| {
            Some(PlacedBody {
                unit,
                unit_type: placed.unit_type,
                pos: placed.pos,
                body: body_of(placed.unit_type)?.at(placed.angle),
            })
        });
        bodies.collect()
    }

    /// The points of its markers of a tag of `spawn_tags`, where its mode's units spawn: what a
    /// walk check takes of them.
    pub fn spawn_points(&self, spawn_tags: &[DeclaredName]) -> Vec<MarkerPoint> {
        let spawns = self
            .markers
            .iter()
            .filter(|marker| marker.tags.iter().any(|tag| spawn_tags.contains(tag)));
        let points = spawns.filter_map(|marker| {
            Some(MarkerPoint {
                name: DeclaredName::new(&marker.name).expect("the map's names are declared"),
                pos: marker.pos?,
            })
        });
        points.collect()
    }
}
