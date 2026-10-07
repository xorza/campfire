use std::ops::Range;

use campfire_math::Num;
use campfire_sim::Position;

use crate::units::relations::Relations;
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::values::body_box::BodyBox;
use crate::vision::sight_cache::{SightCache, Sighting};
use crate::vision::sight_maps::SightMaps;
use crate::vision::vision_grid::VisionGrid;
use crate::vision::vision_groups::VisionGroups;

/// The grid fog's work in a tick, kept between ticks: the vision groups, the bitmaps of the
/// cells each sees, and each sight's runs of cells. `vision::see` drives it over the match's
/// units, a bench over a scene.
#[derive(Debug, Default)]
pub(crate) struct Fog {
    groups: VisionGroups,
    maps: SightMaps,
    sights: SightCache<Sighting>,
    /// The cells each box covers, which only its place and shape decide.
    covers: SightCache<Covering>,
}

/// A box's key among the covers: where it stands, and its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Covering {
    pos: Position,
    body: BodyBox,
}

impl Fog {
    /// Builds the groups of `grid`'s teams again from `relations`, and empty bitmaps for them.
    pub(crate) fn rebuild(&mut self, grid: &VisionGrid, relations: &Relations) {
        self.groups.rebuild(grid.teams, relations);
        self.maps.reset(grid.grid.cells(), self.groups.count());
        self.sights.clear();
        self.covers.clear();
    }

    /// Clears what the tick before revealed.
    pub(crate) fn begin_tick(&mut self) {
        self.maps.begin_tick();
        self.sights.begin_tick();
        self.covers.begin_tick();
    }

    /// Reveals the cells within `range` of a unit of `team` at `pos` to its group, but the cells
    /// of every brush other than the one it stands in, and to its group's detection too when it
    /// `detects`; `slot` is the unit's in the cache of sights.
    pub(crate) fn sight(
        &mut self,
        grid: &VisionGrid,
        slot: usize,
        pos: Position,
        team: Team,
        range: Num,
        detects: bool,
    ) {
        let group = self.groups.of(team);
        let stands = grid
            .grid
            .cell_of(pos)
            .expect("every unit stands within the bounds, which the grid covers");
        let hidden = grid.brush.hidden_from(stands);
        let runs = self.sights.runs(slot, Sighting { pos, range }, |run| {
            grid.grid.spans_within(pos, range, run);
        });
        for cells in runs {
            self.maps.reveal(group, cells.clone(), detects, hidden);
        }
    }

    /// Reveals the cells within `radius` of `pos`, brush included, to `team`'s group.
    pub(crate) fn reveal(&mut self, grid: &VisionGrid, pos: Position, team: Team, radius: Num) {
        let group = self.groups.of(team);
        let maps = &mut self.maps;
        grid.grid.spans_within(pos, radius, |cells| {
            maps.reveal(group, cells, false, None);
        });
    }

    /// The teams that see a unit of `team` at `pos`: its own group's, and those of each group
    /// whose cells hold it, or, for a unit its tags hide, `hidden`, whose detection does. A unit
    /// with a box, `body`, stands in each cell the box covers, which the covers keep for `slot`
    /// while it stands where it stood.
    pub(crate) fn seen_by(
        &mut self,
        grid: &VisionGrid,
        slot: usize,
        pos: Position,
        body: Option<BodyBox>,
        team: Team,
        hidden: bool,
    ) -> TeamSet {
        let mut teams = self.groups.members(self.groups.of(team));
        let cell = grid
            .grid
            .cell_of(pos)
            .expect("every unit stands within the bounds, which the grid covers");
        let runs: &[Range<usize>] = match body {
            Some(body) => self.covers.runs(slot, Covering { pos, body }, |run| {
                grid.grid.box_covers(pos, &body, run);
            }),
            None => &[],
        };
        for group in 0..self.groups.count() {
            let sees = match body {
                Some(_) => runs
                    .iter()
                    .any(|run| self.maps.sees_any(group, run.clone(), hidden)),
                None => self.maps.sees(group, cell, hidden),
            };
            if sees {
                teams = teams.union(self.groups.members(group));
            }
        }
        teams
    }
}
