use std::num::NonZeroU32;
use std::ops::Range;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declarations::Cache;
use campfire_geometry::{BodyBox, RunChange};
use campfire_math::{Num, Vec3};
use campfire_values::{Relations, Team, TeamSet};

use crate::run_cache::RunCache;
use crate::sight_maps::{Looker, SightMaps};
use crate::vision_grid::VisionGrid;
use crate::vision_groups::VisionGroups;

/// The grid fog, kept between ticks: the vision groups, the maps of the cells each sees, each
/// seer's look as the last stage counted it, and the cells each box covers. `vision::see` drives
/// it over the match's units, a bench over a scene.
#[derive(Resource, Debug, Default)]
pub struct Fog {
    /// Whether it holds a Vision stage's sight, which a placement reads; a new or a restored
    /// match's fog holds none until it is filled.
    filled: bool,
    groups: VisionGroups,
    maps: SightMaps,
    /// Each seer's look and its runs, by its slot, as the last stage counted them.
    looks: RunCache<LookKey>,
    /// The runs the last stage's reveals counted, which the next takes back.
    revealed: Vec<Revealed>,
    /// The cells each box covers, which only its place and shape decide.
    covers: RunCache<Covering>,
}

/// One unit's sight, as a stage takes it: its slot, any number its caller keeps for it, its
/// place, team and range, and whether its tags detect.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seer {
    pub(crate) slot: usize,
    pub(crate) position: Vec3,
    pub(crate) team: Team,
    pub(crate) range: Num,
    pub(crate) detects: bool,
}

/// What a seer's sight counts: from where, how far, for which group, whether it detects, and the
/// brush it stands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LookKey {
    position: Vec3,
    range: Num,
    group: usize,
    detects: bool,
    brush: Option<NonZeroU32>,
}

/// A box's key among the covers: where it stands, and its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Covering {
    position: Vec3,
    body: BodyBox,
}

/// A run a reveal counted for its group.
#[derive(Debug)]
struct Revealed {
    group: usize,
    cells: Range<usize>,
}

/// Built again from the match's units, its reveals and its map: a Vision stage fills it, and a
/// fog that holds no stage's sight, as a restored match's, is filled as a tick starts.
impl Cache for Fog {
    fn installed(_: &World) -> Option<Self> {
        Some(Self::default())
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

impl Fog {
    /// Whether it holds a Vision stage's sight.
    pub(crate) const fn is_filled(&self) -> bool {
        self.filled
    }

    /// Whether `team`'s vision group saw the cell of `position` in the last Vision stage, as a
    /// placement asks of its point (04-capabilities/production.md, A build point its team sees).
    pub fn sees_point(&self, grid: &VisionGrid, position: Vec3, team: Team) -> bool {
        let Some(cell) = grid.grid.cell_of(position).filter(|_| self.filled) else {
            return false;
        };
        self.maps.sees(self.groups.of(team), cell..cell + 1, false)
    }

    /// Builds the groups of `grid`'s teams again from `relations`, and empty maps for them.
    pub fn rebuild(&mut self, grid: &VisionGrid, relations: &Relations) {
        self.groups.rebuild(grid.teams, relations);
        self.maps.reset(grid.grid.cells(), self.groups.count());
        self.looks.clear();
        self.revealed.clear();
        self.covers.clear();
    }

    /// Counts what `seers` see in place of what the last stage's seers and reveals saw: each
    /// reveals the cells within its range to its group, but the cells of every brush other than
    /// the one it stands in, and to its group's detection too when it detects. A seer that looks
    /// as it did costs a compare and a copy of its runs, and one the stage does not see takes its
    /// sight back.
    pub(crate) fn see(&mut self, grid: &VisionGrid, seers: impl IntoIterator<Item = Seer>) {
        self.begin_stage();
        for seer in seers {
            self.sight(grid, seer);
        }
        self.end_stage(grid);
    }

    /// Takes back the last stage's reveals.
    fn begin_stage(&mut self) {
        self.filled = true;
        self.looks.begin_tick();
        for Revealed { group, cells } in self.revealed.drain(..) {
            self.maps
                .look(Looker::reveal(group), cells, RunChange::Lost);
        }
        self.covers.begin_tick();
    }

    /// Counts `seer`'s look, against the one its slot held.
    fn sight(&mut self, grid: &VisionGrid, seer: Seer) {
        let stands = grid
            .grid
            .cell_of(seer.position)
            .expect("every unit stands within the bounds, which the grid covers");
        let key = LookKey {
            position: seer.position,
            range: seer.range,
            group: self.groups.of(seer.team),
            detects: seer.detects,
            brush: grid.brush.brush_of(stands),
        };
        let runs = self.looks.runs(seer.slot, key, |run| {
            grid.grid.spans_within(key.position, key.range, run);
        });
        let maps = &mut self.maps;
        match runs.key_before {
            Some(before) if before == key => {}
            Some(before)
                if (before.group, before.detects, before.brush)
                    == (key.group, key.detects, key.brush) =>
            {
                let looker = key.looker(grid);
                grid.grid
                    .changed_runs(runs.before, runs.now, |cells, change| {
                        maps.look(looker, cells, change);
                    });
            }
            Some(before) => {
                maps.look_runs(before.looker(grid), runs.before, RunChange::Lost);
                maps.look_runs(key.looker(grid), runs.now, RunChange::Gained);
            }
            None => maps.look_runs(key.looker(grid), runs.now, RunChange::Gained),
        }
    }

    /// Takes back the sight of each seer the stage did not see, as one that died or lost its
    /// sight.
    fn end_stage(&mut self, grid: &VisionGrid) {
        let maps = &mut self.maps;
        self.looks.end_tick(|key, runs| {
            maps.look_runs(key.looker(grid), runs, RunChange::Lost);
        });
    }

    /// Reveals the cells within `radius` of `position`, brush included, to `team`'s group, in the
    /// stage `see` began.
    pub(crate) fn reveal(&mut self, grid: &VisionGrid, position: Vec3, team: Team, radius: Num) {
        let group = self.groups.of(team);
        let looker = Looker::reveal(group);
        let (maps, revealed) = (&mut self.maps, &mut self.revealed);
        grid.grid.spans_within(position, radius, |cells| {
            maps.look(looker, cells.clone(), RunChange::Gained);
            revealed.push(Revealed { group, cells });
        });
    }

    /// The teams that see a unit of `team` at `position`: its own group's, and those of each group
    /// whose cells hold it, or, for a unit its tags hide, `hidden`, whose detection does. A unit
    /// with a box, `body`, stands in each cell the box covers, which the covers keep for `slot`
    /// while it stands where it stood.
    pub(crate) fn seen_by(
        &mut self,
        grid: &VisionGrid,
        slot: usize,
        position: Vec3,
        body: Option<BodyBox>,
        team: Team,
        hidden: bool,
    ) -> TeamSet {
        let mut teams = self.groups.members(self.groups.of(team));
        let cell = grid
            .grid
            .cell_of(position)
            .expect("every unit stands within the bounds, which the grid covers");
        let runs: &[Range<usize>] = match body {
            Some(body) => {
                let covering = Covering { position, body };
                let runs = self.covers.runs(slot, covering, |run| {
                    grid.grid.box_covers(position, &body, run);
                });
                runs.now
            }
            None => &[],
        };
        for group in 0..self.groups.count() {
            let sees = match body {
                Some(_) => runs
                    .iter()
                    .any(|run| self.maps.sees(group, run.clone(), hidden)),
                None => self.maps.sees(group, cell..cell + 1, hidden),
            };
            if sees {
                teams = teams.union(self.groups.members(group));
            }
        }
        teams
    }
}

impl LookKey {
    /// Whose sight it counts, on `grid`.
    fn looker(self, grid: &VisionGrid) -> Looker<'_> {
        Looker {
            group: self.group,
            detects: self.detects,
            hidden: grid.brush.hidden_in(self.brush),
        }
    }
}

#[cfg(test)]
mod tests;
