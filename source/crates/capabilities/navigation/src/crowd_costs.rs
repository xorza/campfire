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
/// center than the two radii, of those a walker of the team may move and of those it may not. A
/// walker pays nothing for an enemy, which it fights or meets, as Zero Hour's pathfinder prices
/// only allies. Each tick counts again only the units whose body, place, team or standing changed
/// since the last, against the stamps it keeps, so a crowd at rest costs a pass over the units and
/// no write.
#[derive(Resource, Debug)]
pub(crate) struct CrowdCosts {
    grid: Grid,
    kinds: Vec<KindCosts>,
    /// The units as the counts hold them, by stable id, and the buffer the next tick's are read
    /// into.
    stamps: Vec<Stamp>,
    fresh: Vec<Stamp>,
}

/// One kind of walker's counts, a team's at a time, by team.
#[derive(Debug)]
struct KindCosts {
    walker: Walker,
    teams: Vec<TeamCosts>,
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

/// A unit of a team that stands, as the costs count it: its body, its team, and how a walker of
/// its team takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub(crate) body: IndexedBody,
    pub(crate) team: Team,
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

/// What a walker's route makes of the cells a unit hemmed in holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HemmedUnits {
    /// It pays for them as for any unit it may not move.
    Priced,
    /// It may not enter them, as a stuck walker's route
    /// (04-capabilities/navigation.md, A friend that cannot yield is firm, and a stuck walker's
    /// route walls it).
    Walled,
}

/// Whether a unit's stamp adds to the counts or takes from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Add,
    Take,
}

/// What a walker of one kind and team pays to enter a cell of the grid, past the step, and
/// whether it may enter the cells a unit hemmed in holds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CrowdPrice<'a> {
    costs: &'a TeamCosts,
    hemmed: HemmedUnits,
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
        let kinds = walkers
            .iter()
            .map(|&walker| KindCosts {
                walker,
                teams: Vec::new(),
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

    /// What `walker` of `team` pays for its team's units, making of those hemmed in what
    /// `hemmed` says; none for a kind the grid has not, a walker of no team, or a team no unit of
    /// stands for.
    pub(crate) fn price(
        &self,
        walker: Walker,
        team: Option<Team>,
        hemmed: HemmedUnits,
    ) -> Option<CrowdPrice<'_>> {
        let team = team?;
        let kind = self.kinds.iter().find(|kind| kind.walker == walker)?;
        let at = kind.teams.binary_search_by_key(&team, |costs| costs.team);
        let costs = &kind.teams[at.ok()?];
        (costs.units > 0).then_some(CrowdPrice { costs, hemmed })
    }

    /// Adds `stamp`'s unit to the counts of its team for each kind of its layer, or takes it
    /// away, as `change` says.
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
        for kind in self
            .kinds
            .iter_mut()
            .filter(|kind| kind.walker.layer == body.layer)
        {
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
            costs.units = match change {
                Change::Add => costs.units + 1,
                Change::Take => costs.units - 1,
            };
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
                    let bit = cell % columns * rows + cell / columns;
                    if costs.movable[cell] + costs.held[cell] > 0 {
                        costs.stood[bit / 64] |= 1 << (bit % 64);
                    } else {
                        costs.stood[bit / 64] &= !(1 << (bit % 64));
                    }
                }
            });
        }
    }
}

impl<'a> CrowdPrice<'a> {
    /// What entering `cell` costs past the step: `HELD` where a unit the walker may not move
    /// stands, else `MOVABLE` where one it may stands, else nothing.
    pub(crate) fn of(self, cell: usize) -> u32 {
        if self.costs.held[cell] > 0 {
            CrowdCosts::HELD
        } else if self.costs.movable[cell] > 0 {
            CrowdCosts::MOVABLE
        } else {
            0
        }
    }

    /// Whether the walker may not enter `cell`, as a unit hemmed in holds it and the price
    /// walls such units.
    pub(crate) fn walls(self, cell: usize) -> bool {
        self.hemmed == HemmedUnits::Walled && self.costs.hemmed[cell] > 0
    }

    /// The cells some unit holds, one bit a cell, column by column.
    pub(crate) fn stood(self) -> &'a [u64] {
        &self.costs.stood
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
        }
        self.stamps.clear();
    }
}

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
        let [idle, penned, kept] = [(); 3].map(|()| ids.allocate());
        let cell = |x: usize, z: usize| z * 4 + x;
        let cells = [cell(1, 1), cell(2, 2), cell(3, 3), cell(0, 3)];
        let prices = |costs: &CrowdCosts, team| {
            let price = costs.price(walker, Some(team), HemmedUnits::Priced);
            cells.map(|cell| price.map_or(0, |price| price.of(cell)))
        };
        let walls = |costs: &CrowdCosts, hemmed| {
            let price = costs.price(walker, Some(red), hemmed);
            cells.map(|cell| price.is_some_and(|price| price.walls(cell)))
        };
        // No unit stands: no price, and no test of the crowd's cells.
        assert!(
            costs
                .price(walker, Some(red), HemmedUnits::Priced)
                .is_none()
        );
        // A red unit a red walker may move at (1, 1), one hemmed in at (2, 2), and one the mode
        // holds at (0, 3): the first costs a red walker 42, the others 84 each; a blue walker pays
        // for none, and a walker of no team has no price. Only a price that walls the hemmed in
        // walls a cell, the hemmed unit's alone.
        costs.update([
            stamp(idle, at(1, 1), red, Standing::Movable),
            stamp(penned, at(2, 2), red, Standing::Hemmed),
            stamp(kept, at(0, 3), red, Standing::Held),
        ]);
        assert_eq!(prices(&costs, red), [42, 84, 0, 84]);
        assert_eq!(prices(&costs, blue), [0; 4]);
        assert!(costs.price(walker, None, HemmedUnits::Priced).is_none());
        assert_eq!(walls(&costs, HemmedUnits::Priced), [false; 4]);
        assert_eq!(
            walls(&costs, HemmedUnits::Walled),
            [false, true, false, false]
        );
        // The movable unit moves to (3, 3), and the hemmed one walks: their old cells cost
        // nothing again, and no cell is walled.
        costs.update([
            stamp(idle, at(3, 3), red, Standing::Movable),
            stamp(kept, at(0, 3), red, Standing::Held),
        ]);
        assert_eq!(prices(&costs, red), [0, 0, 42, 84]);
        assert_eq!(walls(&costs, HemmedUnits::Walled), [false; 4]);
        // All go, and the costs price nothing again.
        costs.update([]);
        assert!(
            costs
                .price(walker, Some(red), HemmedUnits::Priced)
                .is_none()
        );
        let team = &costs.kinds[0].teams[0];
        assert!(team.stood.iter().all(|&word| word == 0));
        assert!(team.hemmed.iter().all(|&count| count == 0));
    }
}
