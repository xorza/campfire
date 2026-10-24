use std::mem;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declarations::Cache;
use campfire_geometry::Grid;
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
    /// The units as the counts hold them, by stable id, and the buffer the next tick's are read
    /// into.
    stamps: Vec<Stamp>,
    fresh: Vec<Stamp>,
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

    /// Counts of no unit, on `grid`, for each of `walkers`.
    pub(crate) fn new(grid: Grid, walkers: &WalkerKinds) -> Self {
        let cells = grid.cells();
        let kinds = walkers
            .iter()
            .map(|&walker| KindCosts {
                walker,
                teams: Vec::new(),
                units: 0,
                standing: vec![0; cells],
                stood: vec![0; cells.div_ceil(64)],
            })
            .collect();
        Self {
            grid,
            kinds,
            stamps: Vec::new(),
            fresh: Vec::new(),
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
                        self.count(was, Change::Take);
                        self.count(is, Change::Add);
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
        let (columns, rows) = (self.grid.columns(), self.grid.rows());
        let cells = self.grid.cells();
        let step = |count: &mut u16| {
            *count = match change {
                Change::Add => *count + 1,
                Change::Take => *count - 1,
            }
        };
        let mark = |stood: &mut [u64], cell: usize, set: bool| {
            let bit = cell % columns * rows + cell / columns;
            if set {
                stood[bit / 64] |= 1 << (bit % 64);
            } else {
                stood[bit / 64] &= !(1 << (bit % 64));
            }
        };
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
            body.spans_closer(&self.grid, kind.walker.radius, |run| {
                for cell in run {
                    step(&mut kind.standing[cell]);
                    mark(&mut kind.stood, cell, kind.standing[cell] > 0);
                }
            });
            let Some(team) = team else {
                continue;
            };
            let at = match kind.teams.binary_search_by_key(&team, |costs| costs.team) {
                Ok(at) => at,
                Err(at) => {
                    let costs = TeamCosts {
                        team,
                        units: 0,
                        movable: vec![0; cells],
                        held: vec![0; cells],
                        hemmed: vec![0; cells],
                        stood: vec![0; cells.div_ceil(64)],
                    };
                    kind.teams.insert(at, costs);
                    at
                }
            };
            let costs = &mut kind.teams[at];
            costs.units = units(costs.units);
            body.spans_closer(&self.grid, kind.walker.radius, |run| {
                for cell in run {
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
                        cell,
                        costs.movable[cell] + costs.held[cell] > 0,
                    );
                }
            });
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

    /// As the load built it: no unit counted.
    fn clear(&mut self) {
        for kind in &mut self.kinds {
            kind.teams.clear();
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
mod tests {
    use campfire_geometry::{Bounds, Radius, Shape};
    use campfire_math::{Num, Vec3};
    use campfire_sim::IdAllocator;
    use campfire_units::Layer;

    use super::*;

    #[test]
    fn a_cell_costs_by_whether_its_walker_may_move_the_units_of_its_team_in_it() {
        // Cells of 1 m over 4 by 4 m, and a walker of no body: a unit of 0.5 m holds the one
        // cell whose center lies within its radius, the cell of its own center.
        let bounds = Bounds::new([Num::ZERO; 2], [Num::int(4); 2]).unwrap();
        let grid = Grid::new(Num::ONE, bounds).unwrap();
        let walker = Walker {
            layer: Layer::FIRST,
            radius: Num::ZERO,
        };
        let mut costs = CrowdCosts::new(grid, &WalkerKinds::from([walker]));
        let mut ids = IdAllocator::default();
        let at =
            |x: i64, z: i64| Vec3::new(Num::int(x) + Num::HALF, Num::ZERO, Num::int(z) + Num::HALF);
        let stamp = |id, position, team, standing| Stamp {
            body: IndexedBody {
                id,
                position,
                shape: Shape::Circle(Radius::new(Num::HALF).unwrap()),
                layer: Layer::FIRST,
            },
            team,
            standing,
        };
        let [red, blue] = [0, 1].map(Team::new);
        let [idle, penned, kept, foe, stray] = [(); 5].map(|()| ids.allocate());
        let cell = |x: usize, z: usize| z * 4 + x;
        let cells = [
            cell(1, 1),
            cell(2, 2),
            cell(3, 3),
            cell(0, 3),
            cell(3, 0),
            cell(0, 0),
        ];
        let prices = |costs: &CrowdCosts, team| {
            let price = costs.price(walker, Some(team), CrowdWalls::Priced);
            cells.map(|cell| price.map_or(0, |price| price.of(cell)))
        };
        let walls = |costs: &CrowdCosts, walls| {
            let price = costs.price(walker, Some(red), walls);
            cells.map(|cell| price.is_some_and(|price| price.walls(cell)))
        };
        // No unit stands: no price, and no test of the crowd's cells.
        assert!(costs.price(walker, Some(red), CrowdWalls::Priced).is_none());
        assert!(costs.price(walker, Some(red), CrowdWalls::Stuck).is_none());
        // A red unit a red walker may move at (1, 1), one hemmed in at (2, 2), one the mode holds
        // at (0, 3), a blue one at (3, 0) and one of no team at (0, 0): the first costs a red
        // walker 42, the next two 84 each, and the others nothing; a blue walker pays for none of
        // the red, and a walker of no team has no price. Only a stuck walker's route walls cells:
        // the hemmed unit's, the blue unit's and the one of no team's
        // (04-capabilities/navigation.md, A stuck walker's route walls the units of other teams
        // that stand).
        costs.update([
            stamp(idle, at(1, 1), Some(red), Standing::Movable),
            stamp(penned, at(2, 2), Some(red), Standing::Hemmed),
            stamp(kept, at(0, 3), Some(red), Standing::Held),
            stamp(foe, at(3, 0), Some(blue), Standing::Movable),
            stamp(stray, at(0, 0), None, Standing::Held),
        ]);
        assert_eq!(prices(&costs, red), [42, 84, 0, 84, 0, 0]);
        assert_eq!(prices(&costs, blue), [0, 0, 0, 0, 42, 0]);
        assert!(costs.price(walker, None, CrowdWalls::Priced).is_none());
        assert_eq!(walls(&costs, CrowdWalls::Priced), [false; 6]);
        assert_eq!(
            walls(&costs, CrowdWalls::Stuck),
            [false, true, false, false, true, true]
        );
        // The movable unit moves to (3, 3), and the hemmed one, the blue one and the one of no
        // team go: their old cells cost nothing again, and no cell is walled.
        costs.update([
            stamp(idle, at(3, 3), Some(red), Standing::Movable),
            stamp(kept, at(0, 3), Some(red), Standing::Held),
        ]);
        assert_eq!(prices(&costs, red), [0, 0, 42, 84, 0, 0]);
        assert_eq!(walls(&costs, CrowdWalls::Stuck), [false; 6]);
        // All go, and the costs price nothing again.
        costs.update([]);
        assert!(costs.price(walker, Some(red), CrowdWalls::Priced).is_none());
        assert!(costs.price(walker, Some(red), CrowdWalls::Stuck).is_none());
        let kind = &costs.kinds[0];
        let team = &kind.teams[0];
        assert!(team.stood.iter().chain(&kind.stood).all(|&word| word == 0));
        assert!(
            team.hemmed
                .iter()
                .chain(&kind.standing)
                .all(|&count| count == 0)
        );
    }
}
