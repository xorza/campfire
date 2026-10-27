use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section, Sections};
use campfire_geometry::{Grid, Polygon};
use campfire_math::Num;
use campfire_script::MatchNames;
use campfire_units::MapBook;
use campfire_values::{PackageIndex, Team};

use crate::brush_map::BrushMap;
use crate::error::VisionDataError;
use crate::grid_data::GridData;
use crate::vision_grid::VisionGrid;

/// The map's vision grid; none in a map with no grid, where every unit sees all. Map data, not
/// state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
#[component(immutable)]
pub struct VisionBook(pub Option<VisionGrid>);

impl VisionBook {
    pub const fn grid(&self) -> Option<&VisionGrid> {
        self.0.as_ref()
    }

    /// Whether the map has a grid, which what sees needs.
    pub(crate) const fn has_grid(&self) -> bool {
        self.0.is_some()
    }
}

impl Book for VisionBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

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
    /// that has no place on the map, points that make no simple polygon, more teams than a grid
    /// holds, or more cells times teams than its counts may take.
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
        if grid.cells().saturating_mul(teams) > VisionGrid::MAX_CELL_TEAMS {
            return Err(VisionDataError::TooLarge);
        }
        Ok(Self(Some(VisionGrid {
            brush: BrushMap::new(&grid, &areas),
            grid,
            teams,
            shroud: data.shroud,
            neutral_in_fog: data.neutral_in_fog,
        })))
    }
}

#[cfg(test)]
mod tests;
