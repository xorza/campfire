use campfire_declare::{BookCx, BookError, BookPlace, ByType, Section, Sections};
use campfire_geometry::{Grid, Polygon};
use campfire_math::Num;
use campfire_units::{MapBook, TeamNames};
use campfire_values::PackageIndex;

use crate::brush_map::BrushMap;
use crate::error::VisionDataError;
use crate::grid_data::GridData;
use crate::sight::Sight;
use crate::vision_book::VisionBook;
use crate::vision_data::VisionData;
use crate::vision_grid::VisionGrid;

impl VisionBook {
    /// The rule of a mode that declares vision: its map has a grid, for sight to reveal.
    pub(crate) fn grid_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        if cx.map_section::<GridData>().is_none() {
            let problem = VisionDataError::NoGrid;
            let at = BookPlace::MapSection(GridData::KEY);
            return Err(BookError::rule(PackageIndex::MODE, at, problem));
        }
        Ok(())
    }

    /// The map's vision grid over its bounds, with its brush, for the match's teams; none for a
    /// map of no `[grid]`. An error for a cell that makes no grid of the bounds, a brush point
    /// that has no place on the map, points that make no simple polygon, or more teams than a
    /// grid holds.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let map = cx.book::<MapBook>()?;
        let teams = cx.fact::<TeamNames>()?.0.len();
        let book = Self::of(map, cx.source().map, teams);
        let at = BookPlace::MapSection(GridData::KEY);
        book.map_err(|problem| BookError::rule(PackageIndex::MODE, at, problem))
    }

    /// The book of a map of `sections`, as `map` places its points, for `teams` teams; an error
    /// as `build` gives.
    pub fn of(map: &MapBook, sections: &Sections, teams: usize) -> Result<Self, VisionDataError> {
        let Some(data) = sections.get::<GridData>() else {
            return Ok(Self(None));
        };
        let grid = Grid::new(data.cell, map.bounds).ok_or(VisionDataError::Grid)?;
        let mut areas = Vec::with_capacity(data.brush.len());
        for (brush, data) in data.brush.iter().enumerate() {
            let points = data.points.iter().map(|point| {
                let at = map.pos(*point)?;
                Ok([at.x, at.z])
            });
            let points = points.collect::<Result<Vec<[Num; 2]>, VisionDataError>>()?;
            let area = Polygon::new(points)
                .map_err(|problem| VisionDataError::Brush { brush, problem })?;
            areas.push(area);
        }
        if teams > VisionGrid::MAX_TEAMS {
            return Err(VisionDataError::TooManyTeams);
        }
        Ok(Self(Some(VisionGrid {
            brush: BrushMap::new(&grid, &areas),
            grid,
            teams,
        })))
    }
}

impl Sight {
    /// The sight of each unit type with a `vision` section, a delivery type's among them.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let mut book = ByType::default();
        for unit in cx.types() {
            if let Some(vision) = unit.data.section::<VisionData>() {
                book.set(unit.unit_type, vision.sight);
            }
        }
        Ok(book)
    }
}

#[cfg(test)]
mod tests {
    use campfire_geometry::{Bounds, Metric, PolygonError};
    use campfire_units::{Ground, MapPoint, MapPointError};

    use super::*;
    use crate::grid_data::BrushData;

    #[test]
    fn a_brush_loads_only_as_a_simple_polygon_within_the_bounds() {
        let bounds = Bounds::new([Num::ZERO; 2], [Num::int(4); 2]).unwrap();
        let map = MapBook {
            metric: Metric::Planar,
            bounds,
            ground: Ground::default(),
        };
        let with = |points: &[(i64, i64)]| {
            let sections = Sections::default().with(GridData {
                cell: Num::ONE,
                brush: vec![BrushData {
                    points: points
                        .iter()
                        .map(|&(x, z)| MapPoint::ground(x, z))
                        .collect(),
                }],
            });
            VisionBook::of(&map, &sections, 2)
        };
        let fault = |problem| Err(VisionDataError::Brush { brush: 0, problem });
        assert_eq!(
            with(&[(0, 0), (4, 0), (0, 5)]),
            Err(VisionDataError::Point(MapPointError::OutOfBounds))
        );
        assert_eq!(with(&[(0, 0), (4, 0)]), fault(PolygonError::TooFewPoints));
        let crossed = PolygonError::EdgesMeet {
            first: 0,
            second: 2,
        };
        assert_eq!(with(&[(0, 0), (4, 4), (4, 0), (0, 4)]), fault(crossed));
        let triangle = with(&[(0, 0), (4, 0), (0, 4)]).unwrap();
        let grid = triangle.grid().unwrap();
        assert_eq!((grid.teams, grid.grid.cells()), (2, 16));
        // A map with no `[grid]` has no vision grid.
        assert_eq!(
            VisionBook::of(&map, &Sections::default(), 2),
            Ok(VisionBook(None))
        );
    }
}
