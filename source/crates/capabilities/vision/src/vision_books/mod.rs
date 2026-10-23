use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, Section, Sections};
use campfire_geometry::{Grid, Polygon};
use campfire_math::Num;
use campfire_units::MapBook;
use campfire_values::{PackageIndex, Team, UnitType};

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
        let teams = cx.names::<Team>()?.len();
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
                let at = map.checked_position(*point)?;
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
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let mut book = ByIdBuilder::default();
        for unit in cx.types() {
            if let Some(vision) = unit.data.section::<VisionData>() {
                book.set(unit.unit_type, vision.sight);
            }
        }
        Ok(book.build())
    }
}

#[cfg(test)]
mod tests;
