use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Sections};
use campfire_geometry::{Grid, GridSearch};
use campfire_math::Vec3;
use campfire_script::MatchNames;
use campfire_units::{MapBook, MapPoint};
use campfire_values::{Names, PackageIndex, PathId};

use crate::error::NavigationDataError;
use crate::map_navigation_data::MapNavigationData;
use crate::map_paths_data::MapPathsData;
use crate::paths::{PathOf, Paths};

/// The map's paths, which walkers go along, and the cells units plan routes over. Map data, not
/// state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct NavigationBook {
    pub paths: Paths,
    /// The cells units plan routes over, when the map has them.
    pub pathing: Option<Grid>,
}

impl NavigationBook {
    /// The names of the map's paths, which the load resolves them by.
    pub(crate) fn path_names(cx: &BookCx<'_>) -> Result<Arc<Names<PathId>>, BookError> {
        Ok(Arc::new(cx.book::<Self>()?.paths.names().clone()))
    }
}

impl Book for NavigationBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, names: &mut MatchNames) {
        names.set_table(self.paths.shared_names());
    }
}

impl NavigationBook {
    /// The map's paths and the cells units plan routes over, when the map has them; an error
    /// for a path of no waypoint or another's name, a point that has no place on the map, or a
    /// grid that makes no grid of the bounds.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        GridSearch::prepare();
        let map = cx.book::<MapBook>()?;
        let book = Self::of(map, cx.source().map);
        book.map_err(|problem| BookError::rule(PackageIndex::MODE, BookPlace::Map, problem))
    }

    /// The book of a map of `sections`, as `map` places its points; an error as `build` gives.
    pub fn of(map: &MapBook, sections: &Sections) -> Result<Self, NavigationDataError> {
        let position = |point: &MapPoint| {
            map.checked_position(*point)
                .map_err(NavigationDataError::Point)
        };
        let mut names = Names::default();
        let paths = sections.get::<MapPathsData>().into_iter();
        let mut points = Vec::new();
        for path in paths.flat_map(|paths| &paths.0) {
            if names.try_add(path.name.as_str()).is_err() {
                return Err(NavigationDataError::RepeatedPath(path.name.clone()));
            }
            if path.points.is_empty() {
                return Err(NavigationDataError::EmptyPath(path.name.clone()));
            }
            let path_points = path.points.iter().map(position);
            points.push(PathOf {
                points: path_points.collect::<Result<Vec<Vec3>, _>>()?,
                walk: path.walk,
            });
        }
        let pathing = sections.get::<MapNavigationData>().map(|navigation| {
            let grid = Grid::new(navigation.cell, map.bounds);
            grid.ok_or(NavigationDataError::Grid)
        });
        Ok(Self {
            paths: Paths::of(names, &points),
            pathing: pathing.transpose()?,
        })
    }
}
