use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, Fact};
use campfire_navigation::{MarkerPoint, NavigationBook, Paths, PlacedBody};
use campfire_units::{BodyForm, MapPoint, TeamNames};
use campfire_values::{
    Names, NamesError, PackageIndex, Relations, Team, TypeScope, UnitType, VisionShare,
};

use crate::error::ModeError;
use crate::map_data::MapData;
use crate::marker_spec::MarkerSpec;
use crate::mode_data::ModeData;
use crate::mode_teams::ModeTeams;
use crate::placed_unit::{PlacedPath, PlacedUnit};
use crate::relation_data::RelationData;
use crate::team_manifest::TeamManifest;

/// What a mode's map resolves from: the map, the relations of the mode's teams, the teams' names,
/// and the map's paths, when it has them.
#[derive(Debug)]
pub struct MapInput<'a> {
    pub map: &'a MapData,
    pub relations: &'a [RelationData],
    pub teams: Names<Team>,
    pub paths: Option<&'a Paths>,
}

/// The mode's map and the relations of its teams, every name resolved once: the teams' names, the
/// relations the match starts with, the units it places and its markers.
#[derive(Debug)]
pub struct ModeMap {
    /// The mode's teams' names, by team.
    pub teams: Names<Team>,
    pub relations: Relations,
    pub placed: Vec<PlacedUnit>,
    pub markers: Vec<MarkerSpec>,
}

impl Fact for ModeMap {}

impl ModeMap {
    /// The names of `teams`; an error for two of one name, or more than a team index counts.
    pub fn team_names(teams: &[TeamManifest]) -> Result<Names<Team>, ModeError> {
        let mut names = Names::default();
        for team in teams {
            match names.try_add(team.name.as_str()) {
                Ok(_) => {}
                Err(NamesError::Repeated) => {
                    return Err(ModeError::RepeatedTeam(team.name.clone()));
                }
                Err(NamesError::Full) => return Err(ModeError::TooManyTeams),
            }
        }
        Ok(names)
    }

    /// The map the load gives, its placed units' types among the mode's that stand and their
    /// paths among the map's, with the relations of the mode's teams; an error as `of` gives.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let data = cx
            .given::<ModeData>()
            .expect("the load gives the mode's data");
        let map = cx
            .given::<MapData>()
            .expect("the load gives the mode's map");
        let input = MapInput {
            map,
            relations: &data.relations,
            teams: cx.fact::<TeamNames>()?.0.clone(),
            paths: cx.book_if::<NavigationBook>()?.map(|book| &book.paths),
        };
        let unit_type = |name: &str| {
            let unit_type = cx.unit_type(TypeScope::Mode, name);
            unit_type.filter(|&unit_type| cx.type_of(unit_type).stands())
        };
        let map = Self::of(input, unit_type);
        map.map_err(|error| BookError::rule(PackageIndex::MODE, BookPlace::Map, error))
    }

    /// The map of `input`, its placed units' types resolved by `unit_type`, which knows the mode's
    /// types that stand. An error for what it names that the mode does not have: a relation of a
    /// team to itself, of a team the mode lacks, or of a pair named before; a placed unit of a
    /// type, team or path it lacks, or that walks from an end of no path; a marker of another's
    /// name, a team it lacks, or with a point and a region or a region outside the bounds; and
    /// any point that does not fit its metric or its bounds.
    pub fn of(
        input: MapInput<'_>,
        unit_type: impl Fn(&str) -> Option<UnitType>,
    ) -> Result<Self, ModeError> {
        let MapInput {
            map,
            relations,
            teams,
            paths,
        } = input;
        let team = |name: &DeclaredName| {
            let team = teams.named(name.as_str());
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
            let vision = if relation.vision {
                VisionShare::Shared
            } else {
                VisionShare::Apart
            };
            resolved.set(pair[0], pair[1], relation.relation, vision);
        }
        let mut placed = Vec::with_capacity(map.units.len());
        for unit in &map.units {
            let of_type = unit_type(unit.unit_type.as_str())
                .ok_or_else(|| ModeError::UnknownUnitType(unit.unit_type.clone()))?;
            let path = match (&unit.path, unit.from) {
                (Some(path), from) => {
                    let id = paths.and_then(|paths| paths.names().named(path.as_str()));
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
                position: map.placed_at(&unit.position, unit.height)?,
                angle: unit.angle,
            });
        }
        let markers = Self::markers(map, team)?;
        Ok(Self {
            teams,
            relations: resolved,
            placed,
            markers,
        })
    }

    /// The names of the match's teams, in the manifest's order the load gives; an error for two
    /// of one name, or more than a team index counts.
    pub(crate) fn team_fact(cx: &BookCx<'_>) -> Result<TeamNames, BookError> {
        let teams = cx
            .given::<ModeTeams>()
            .expect("the load gives the mode's teams");
        let names = Self::team_names(&teams.0);
        let names =
            names.map_err(|error| BookError::rule(PackageIndex::MODE, BookPlace::Teams, error))?;
        Ok(TeamNames(names))
    }

    /// The markers of `map`, each with its team as `team` resolves it; an error for two of one
    /// name, a team the mode lacks, or a region that is no box within the bounds or beside a
    /// point.
    fn markers(
        map: &MapData,
        team: impl Fn(&DeclaredName) -> Result<Team, ModeError>,
    ) -> Result<Vec<MarkerSpec>, ModeError> {
        let point = |point: &MapPoint| map.position(point);
        let mut markers = Vec::with_capacity(map.markers.len());
        for (at, marker) in map.markers.iter().enumerate() {
            if map.markers[..at]
                .iter()
                .any(|other| other.name == marker.name)
            {
                return Err(ModeError::RepeatedMarker(marker.name.clone()));
            }
            let marker_team = marker.team.as_ref().map(&team).transpose()?;
            let region = marker.region.map(|region| {
                region
                    .region(map.metric(), map.bounds())
                    .filter(|_| marker.position.is_none())
                    .ok_or_else(|| ModeError::Region(marker.name.clone()))
            });
            let region = region.transpose()?;
            markers.push(MarkerSpec {
                name: marker.name.clone(),
                tags: marker.tags.iter().cloned().collect(),
                position: marker.position.as_ref().map(point).transpose()?,
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
                position: placed.position,
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
                name: marker.name.clone(),
                position: marker.position?,
            })
        });
        points.collect()
    }
}

#[cfg(test)]
mod tests;
