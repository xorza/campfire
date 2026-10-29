use std::mem;
use std::ops::Range;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declarations::Cache;
use campfire_geometry::{Grid, RunChange};
use campfire_values::Team;

use crate::body_index::IndexedBody;
use crate::walker::Walker;
use crate::walker_kinds::WalkerKinds;

/// The units of each team that stand, as a long route prices the cells they hold for a walker of
/// their team (04-capabilities/navigation.md, A route prices the units on it): for each kind of
/// walker of the pathing grid and each team, how many units' bodies come closer to each cell's
/// center than the two radii, of those a walker of the team may move and of those it may not; and
/// for each kind, how many units of any team or none. A walker pays nothing for an enemy, which it
/// fights or meets, as Zero Hour's pathfinder prices only allies, but a stuck walker's route walls
/// the units of other teams (04-capabilities/navigation.md, A stuck walker's route walls the units
/// of other teams that stand). Each tick counts again only the units whose body, place, team or
/// standing changed since the last, against the stamps it keeps, so a crowd at rest costs a pass
/// over the units and no write.
#[derive(Resource, Debug)]
pub(crate) struct CrowdCosts {
    grid: Grid,
    kinds: Vec<KindCosts>,
    /// Whether the load made every team's counts, so a unit of another team is a bug.
    teams_made: bool,
    /// The units as the counts hold them, by stable id, and the buffer the next tick's are read
    /// into.
    stamps: Vec<Stamp>,
    fresh: Vec<Stamp>,
    /// A moved unit's runs of cells before and after, by row, for `recount`.
    old_runs: Vec<Range<usize>>,
    new_runs: Vec<Range<usize>>,
}

/// One kind of walker's counts, a team's at a time, by team, and of every unit that stands, of
/// any team or none: how many, by cell, and the cells some unit holds, one bit a cell, column by
/// column.
#[derive(Debug)]
struct KindCosts {
    walker: Walker,
    teams: Vec<TeamCosts>,
    units: u32,
    standing: Vec<u16>,
    stood: Vec<u64>,
}

/// One team's units, as a walker of one kind counts them, by cell: those it may move, those it
/// may not, the hemmed in among them, and the hemmed in alone.
#[derive(Debug)]
struct TeamCosts {
    team: Team,
    /// The units counted, so a team no unit of stands for prices nothing and costs no test.
    units: u32,
    movable: Vec<u16>,
    held: Vec<u16>,
    hemmed: Vec<u16>,
    /// The cells some unit holds, one bit a cell, column by column, as a line's test reads the
    /// walls'.
    stood: Vec<u64>,
}

impl TeamCosts {
    /// A team's counts of no unit, on a grid of `cells` cells.
    fn new(team: Team, cells: usize) -> Self {
        Self {
            team,
            units: 0,
            movable: vec![0; cells],
            held: vec![0; cells],
            hemmed: vec![0; cells],
            stood: vec![0; cells.div_ceil(64)],
        }
    }

    /// Counts no unit.
    fn clear(&mut self) {
        self.units = 0;
        self.movable.fill(0);
        self.held.fill(0);
        self.hemmed.fill(0);
        self.stood.fill(0);
    }
}

/// A unit that stands, as the costs count it: its body, its team, if any, and how a walker of its
/// team takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub(crate) body: IndexedBody,
    pub(crate) team: Option<Team>,
    pub(crate) standing: Standing,
}

/// How a walker of a unit's team takes the unit that stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Standing {
    /// It may move the unit aside.
    Movable,
    /// It may not, as the unit is firm or the mode moves no unit of the team.
    Held,
    /// It may not, as the unit found no place to step aside to.
    Hemmed,
}

/// What a walker's route makes of the units that stand that it may not pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CrowdWalls {
    /// It pays for a friend hemmed in as for any it may not move, and nothing for a unit of
    /// another team.
    Priced,
    /// It may not enter the cells a friend hemmed in or a unit of another team holds, as a stuck
    /// walker's route (04-capabilities/navigation.md, A friend that cannot yield is firm, and a
    /// stuck walker's route walls it; 04-capabilities/navigation.md, A stuck walker's route walls
    /// the units of other teams that stand).
    Stuck,
}

/// Whether a unit's stamp adds to the counts or takes from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Add,
    Take,
}

/// What a walker of one kind and team pays to enter a cell of the grid, past the step, by its
/// team's counts, if any stands, and which cells it may not enter, by its kind's and `walls`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CrowdPrice<'a> {
    team: Option<&'a TeamCosts>,
    kind: &'a KindCosts,
    walls: CrowdWalls,
}

impl CrowdCosts {
    /// What a cell a unit the walker may move holds costs past its step: three diagonal steps, as
    /// Zero Hour's pathfinder prices a cell an ally that stands holds.
    pub(crate) const MOVABLE: u32 = 42;

    /// What a cell a unit of the walker's team it may not move holds costs past its step: twice a
    /// movable unit's, as the walker must go round it, by its short route at worst.
    pub(crate) const HELD: u32 = 84;

    /// Counts of no unit, on `grid`, for each of `walkers` and each of `teams`, the match's, made
    /// now, so no tick makes one; with no team, as in a test's match of no mode, a team's counts
    /// are made as its first unit stands.
    pub(crate) fn new(
        grid: Grid,
        walkers: &WalkerKinds,
        teams: impl IntoIterator<Item = Team>,
    ) -> Self {
        let cells = grid.cells();
        let mut teams: Vec<Team> = teams.into_iter().collect();
        teams.sort_unstable();
        teams.dedup();
        let kinds = walkers
            .iter()
            .map(|&walker| KindCosts {
                walker,
                teams: teams
                    .iter()
                    .map(|&team| TeamCosts::new(team, cells))
                    .collect(),
                units: 0,
                standing: vec![0; cells],
                stood: vec![0; cells.div_ceil(64)],
            })
            .collect();
        Self {
            grid,
            kinds,
            teams_made: !teams.is_empty(),
            stamps: Vec::new(),
            fresh: Vec::new(),
            old_runs: Vec::new(),
            new_runs: Vec::new(),
        }
    }

    /// Counts the units that stand now, `standing`, in place of those it counted: it takes away
    /// each unit it counted whose stamp changed or that stands no more, and adds each whose stamp
    /// is new.
    pub(crate) fn update(&mut self, standing: impl IntoIterator<Item = Stamp>) {
        let mut fresh = mem::take(&mut self.fresh);
        fresh.clear();
        fresh.extend(standing);
        fresh.sort_unstable_by_key(|stamp| stamp.body.id);
        let stamps = mem::take(&mut self.stamps);
        let (mut old, mut new) = (stamps.iter().peekable(), fresh.iter().peekable());
        loop {
            match (old.peek(), new.peek()) {
                (None, None) => break,
                (Some(&&was), Some(&&is)) if was.body.id == is.body.id => {
                    if was != is {
                        let moved = was.team == is.team
                            && was.standing == is.standing
                            && was.body.layer == is.body.layer;
                        if moved {
                            self.recount(was, is);
                        } else {
                            self.count(was, Change::Take);
                            self.count(is, Change::Add);
                        }
                    }
                    old.next();
                    new.next();
                }
                (Some(&&was), Some(&&is)) if was.body.id < is.body.id => {
                    self.count(was, Change::Take);
                    old.next();
                }
                (Some(&&was), None) => {
                    self.count(was, Change::Take);
                    old.next();
                }
                (_, Some(&&is)) => {
                    self.count(is, Change::Add);
                    new.next();
                }
            }
        }
        self.fresh = stamps;
        self.stamps = fresh;
    }

    /// What `walker` of `team` pays for its team's units and which cells it may not enter, as
    /// `walls` says; none for a kind the grid has not, and, for a route that walls nothing, for a
    /// walker of no team or a team no unit of stands for, or, for a stuck walker's, with no unit
    /// that stands: a price that changes nothing costs no test.
    pub(crate) fn price(
        &self,
        walker: Walker,
        team: Option<Team>,
        walls: CrowdWalls,
    ) -> Option<CrowdPrice<'_>> {
        let kind = self.kinds.iter().find(|kind| kind.walker == walker)?;
        let team = team.and_then(|team| {
            let at = kind.teams.binary_search_by_key(&team, |costs| costs.team);
            Some(&kind.teams[at.ok()?]).filter(|costs| costs.units > 0)
        });
        let prices = match walls {
            CrowdWalls::Priced => team.is_some(),
            CrowdWalls::Stuck => kind.units > 0,
        };
        prices.then_some(CrowdPrice { team, kind, walls })
    }

    /// Adds `stamp`'s unit to the counts of each kind of its layer, and of its team in each, or
    /// takes it away, as `change` says.
    fn count(&mut self, stamp: Stamp, change: Change) {
        let Stamp {
            body,
            team,
            standing,
        } = stamp;
        let grid = &self.grid;
        let cells = grid.cells();
        let units = |units: u32| match change {
            Change::Add => units + 1,
            Change::Take => units - 1,
        };
        for kind in self
            .kinds
            .iter_mut()
            .filter(|kind| kind.walker.layer == body.layer)
        {
            kind.units = units(kind.units);
            let team = team.map(|team| {
                let at = match kind.teams.binary_search_by_key(&team, |costs| costs.team) {
                    Ok(at) => at,
                    Err(at) => {
                        debug_assert!(
                            !self.teams_made,
                            "a unit of {team:?} stands, a team the match lacks"
                        );
                        kind.teams.insert(at, TeamCosts::new(team, cells));
                        at
                    }
                };
                kind.teams[at].units = units(kind.teams[at].units);
                at
            });
            body.spans_closer(grid, kind.walker.radius, |run| {
                Self::count_cells(grid, kind, team, standing, run, change);
            });
        }
    }

    /// Moves `was`'s unit to `is`, of its team and standing on its layer: counts only the cells
    /// each kind's footprint gained or lost, row by row, as the take of the one and the add of
    /// the other leave every other cell's counts as they were.
    fn recount(&mut self, was: Stamp, is: Stamp) {
        debug_assert!(
            was.team == is.team && was.standing == is.standing && was.body.layer == is.body.layer,
            "a move keeps the unit's team, standing and layer"
        );
        let grid = &self.grid;
        let (old, new) = (&mut self.old_runs, &mut self.new_runs);
        for kind in self
            .kinds
            .iter_mut()
            .filter(|kind| kind.walker.layer == is.body.layer)
        {
            old.clear();
            new.clear();
            was.body
                .spans_closer(grid, kind.walker.radius, |run| old.push(run));
            is.body
                .spans_closer(grid, kind.walker.radius, |run| new.push(run));
            let team = is.team.map(|team| {
                kind.teams
                    .binary_search_by_key(&team, |costs| costs.team)
                    .expect("a counted unit's team has costs")
            });
            grid.changed_runs(old, new, |cells, how| {
                let change = match how {
                    RunChange::Lost => Change::Take,
                    RunChange::Gained => Change::Add,
                };
                Self::count_cells(grid, kind, team, is.standing, cells, change);
            });
        }
    }

    /// Adds a unit of `standing` to `cells`, one run of one row, in `kind`'s counts and in those
    /// of its team at `team` among the kind's, if any, or takes it away, as `change` says, and
    /// marks each cell some unit holds.
    fn count_cells(
        grid: &Grid,
        kind: &mut KindCosts,
        team: Option<usize>,
        standing: Standing,
        cells: Range<usize>,
        change: Change,
    ) {
        let (columns, rows) = (grid.columns(), grid.rows());
        let step = |count: &mut u16| {
            *count = match change {
                Change::Add => *count + 1,
                Change::Take => *count - 1,
            }
        };
        let mark = |stood: &mut [u64], bit: usize, set: bool| {
            if set {
                stood[bit / 64] |= 1 << (bit % 64);
            } else {
                stood[bit / 64] &= !(1 << (bit % 64));
            }
        };
        // A run is one row, so its cells' column-major bits step by the rows from its first's.
        let bits = (cells.start % columns * rows + cells.start / columns..).step_by(rows);
        for (cell, bit) in cells.zip(bits) {
            step(&mut kind.standing[cell]);
            mark(&mut kind.stood, bit, kind.standing[cell] > 0);
            let Some(team) = team else {
                continue;
            };
            let costs = &mut kind.teams[team];
            match standing {
                Standing::Movable => step(&mut costs.movable[cell]),
                Standing::Held => step(&mut costs.held[cell]),
                Standing::Hemmed => {
                    step(&mut costs.held[cell]);
                    step(&mut costs.hemmed[cell]);
                }
            }
            mark(
                &mut costs.stood,
                bit,
                costs.movable[cell] + costs.held[cell] > 0,
            );
        }
    }
}

impl<'a> CrowdPrice<'a> {
    /// What entering `cell` costs past the step: `HELD` where a unit the walker may not move
    /// stands, else `MOVABLE` where one it may stands, else nothing.
    pub(crate) fn of(self, cell: usize) -> u32 {
        let Some(costs) = self.team else {
            return 0;
        };
        if costs.held[cell] > 0 {
            CrowdCosts::HELD
        } else if costs.movable[cell] > 0 {
            CrowdCosts::MOVABLE
        } else {
            0
        }
    }

    /// Whether the walker may not enter `cell`: on a stuck walker's route, as a friend hemmed in
    /// holds it, or more units than its team's do, so one of another team or none.
    pub(crate) fn walls(self, cell: usize) -> bool {
        if self.walls == CrowdWalls::Priced {
            return false;
        }
        let (own, hemmed) = self.team.map_or((0, false), |costs| {
            (
                costs.movable[cell] + costs.held[cell],
                costs.hemmed[cell] > 0,
            )
        });
        hemmed || self.kind.standing[cell] > own
    }

    /// The cells some unit holds, one bit a cell, column by column: its team's on a route that
    /// walls nothing, every unit's on a stuck walker's, so no line crosses a walled cell.
    pub(crate) fn stood(self) -> &'a [u64] {
        match (self.walls, self.team) {
            (CrowdWalls::Priced, Some(costs)) => &costs.stood,
            _ => &self.kind.stood,
        }
    }
}

impl Cache for CrowdCosts {
    fn installed(_: &World) -> Option<Self> {
        None
    }

    /// As the load built it: no unit counted, and the counts of the teams the load made kept.
    fn clear(&mut self) {
        for kind in &mut self.kinds {
            if self.teams_made {
                kind.teams.iter_mut().for_each(TeamCosts::clear);
            } else {
                kind.teams.clear();
            }
            kind.units = 0;
            kind.standing.fill(0);
            kind.stood.fill(0);
        }
        self.stamps.clear();
    }
}

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
