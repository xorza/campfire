use std::collections::BTreeMap;

use bevy_ecs::world::World;
use campfire_sim::Position;

use crate::mode::error::ModeError;
use crate::mode::map_data::{MapData, MapPoint};
use crate::mode::mode_data::ModeParam;
use crate::mode::placed_unit::{PlacedPath, PlacedUnit};
use crate::mode::relation_data::RelationData;
use crate::mode::team_manifest::TeamManifest;
use crate::navigation::Navigation;
use crate::navigation::navigation_rules::NavigationRules;
use crate::navigation::paths::Paths;
use crate::navigation::walker::Walker;
use crate::navigation::wall::Wall;
use crate::units::relations::Relations;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::bounds::Bounds;
use crate::values::declared_name::DeclaredName;
use crate::values::grid::Grid;
use crate::values::metric::Metric;
use crate::values::name_list::NameList;
use crate::values::polygon::Polygon;
use crate::values::region::Region;
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
            if let Some(region) = marker.region
                && (marker.pos.is_some() || !region.holds(map.metric, map.bounds))
            {
                return Err(ModeError::Region(marker.name.clone()));
            }
            let region = marker.region.map(|region| {
                let [min, max] = [region.min, region.max].map(|corner| {
                    let at = corner
                        .position()
                        .expect("the region holds within the bounds")
                        .get();
                    [at.x, at.z]
                });
                Region::new(min, max)
            });
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
