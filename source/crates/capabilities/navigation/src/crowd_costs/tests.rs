use campfire_common::{SegmentSeed, Tick};
use campfire_geometry::{Bounds, Radius, Shape};
use campfire_math::{Num, Rng, RngStream, Vec3};
use campfire_sim::IdAllocator;
use campfire_units::Layer;

use super::*;

#[test]
fn counts_kept_through_moves_are_those_counted_afresh() {
    // 30 units of three teams or none, of radii 0.3 to 1.5 m, on a grid of 0.5 m cells over
    // 20 by 12 m, for walkers of two radii: 40 updates move, turn and change units, and the
    // counts each leaves are those a new count of its stamps makes. The kept counts start with
    // the three teams' made, given out of order and one twice, so no update makes one; the new
    // counts make each as its first unit stands.
    let bounds = Bounds::new([Num::ZERO; 2], [Num::int(20), Num::int(12)]).unwrap();
    let grid = Grid::new(Num::HALF, bounds).unwrap();
    let walkers = [Num::ZERO, Num::int(3) / 4].map(|radius| Walker {
        layer: Layer::FIRST,
        radius,
    });
    let teams = [2, 0, 1, 0].map(Team::new);
    let mut costs = CrowdCosts::new(grid, &WalkerKinds::from(walkers), teams);
    let made = |costs: &CrowdCosts| -> Vec<Vec<(Team, *const u16)>> {
        let kinds = costs.kinds.iter();
        kinds
            .map(|kind| {
                let teams = kind.teams.iter();
                teams
                    .map(|team| (team.team, team.movable.as_ptr()))
                    .collect()
            })
            .collect()
    };
    let before = made(&costs);
    let order: Vec<Vec<Team>> = before
        .iter()
        .map(|kind| kind.iter().map(|&(team, _)| team).collect())
        .collect();
    assert_eq!(order, vec![[0, 1, 2].map(Team::new).to_vec(); 2]);
    let mut ids = IdAllocator::default();
    let mut rng = Rng::new(&SegmentSeed::ZERO, RngStream::new("crowd"), 3, Tick::ZERO);
    let tenths = |rng: &mut Rng, low: usize, high: usize| {
        Num::int(i64::try_from(low + rng.pick(high - low)).unwrap()) / 10
    };
    let mut units: Vec<Stamp> = (0..30)
        .map(|_| Stamp {
            body: IndexedBody {
                id: ids.allocate(),
                position: Vec3::new(
                    tenths(&mut rng, 20, 180),
                    Num::ZERO,
                    tenths(&mut rng, 20, 100),
                ),
                shape: Shape::Circle(Radius::new(tenths(&mut rng, 3, 16)).unwrap()),
                layer: Layer::FIRST,
            },
            team: [
                None,
                Some(Team::new(0)),
                Some(Team::new(1)),
                Some(Team::new(2)),
            ][rng.pick(4)],
            standing: [Standing::Movable, Standing::Held, Standing::Hemmed][rng.pick(3)],
        })
        .collect();
    for _ in 0..40 {
        for unit in &mut units {
            match rng.pick(8) {
                0 => {
                    unit.standing =
                        [Standing::Movable, Standing::Held, Standing::Hemmed][rng.pick(3)];
                }
                1 => unit.team = [None, Some(Team::new(0)), Some(Team::new(2))][rng.pick(3)],
                2..=5 => {
                    let step = |rng: &mut Rng| tenths(rng, 0, 9) - Num::int(4) / 10;
                    unit.body.position += Vec3::new(step(&mut rng), Num::ZERO, step(&mut rng));
                }
                _ => {}
            }
        }
        let standing = units
            .iter()
            .copied()
            .filter(|unit| unit.body.id.get() % 7 != 0);
        costs.update(standing.clone());
        let mut fresh = CrowdCosts::new(costs.grid, &WalkerKinds::from(walkers), []);
        fresh.update(standing);
        for (kept, made) in costs.kinds.iter().zip(&fresh.kinds) {
            assert_eq!(
                (kept.units, &kept.standing, &kept.stood),
                (made.units, &made.standing, &made.stood)
            );
            let teams = |kind: &KindCosts| -> Vec<_> {
                kind.teams
                    .iter()
                    .filter(|costs| costs.units > 0)
                    .map(|costs| {
                        (
                            costs.team,
                            costs.units,
                            costs.movable.clone(),
                            costs.held.clone(),
                            costs.hemmed.clone(),
                            costs.stood.clone(),
                        )
                    })
                    .collect()
            };
            assert_eq!(teams(kept), teams(made));
        }
    }
    assert_eq!(made(&costs), before);
    costs.clear();
    assert_eq!(made(&costs), before);
    let counts = |costs: &CrowdCosts| -> Vec<_> {
        let kinds = costs.kinds.iter();
        kinds
            .map(|kind| {
                let teams = kind.teams.iter().map(|team| {
                    let TeamCosts {
                        team,
                        units,
                        movable,
                        held,
                        hemmed,
                        stood,
                    } = team;
                    (
                        *team,
                        *units,
                        movable.clone(),
                        held.clone(),
                        hemmed.clone(),
                        stood.clone(),
                    )
                });
                (
                    kind.units,
                    kind.standing.clone(),
                    kind.stood.clone(),
                    teams.collect::<Vec<_>>(),
                )
            })
            .collect()
    };
    let made_now = CrowdCosts::new(grid, &WalkerKinds::from(walkers), teams);
    assert_eq!(counts(&costs), counts(&made_now));
}

#[test]
fn the_cells_units_hold_are_marked_column_by_column() {
    // Cells of 1 m over 7 by 5 m, not square, so a mark by rows or a wrong step shows: a unit
    // of 1.2 m at (2.5, 2.5) holds the cells whose centers lie within 1.2 m of it, three rows
    // of up to three cells; another at (5.5, 1.5) holds three rows past the first's columns.
    let bounds = Bounds::new([Num::ZERO; 2], [Num::int(7), Num::int(5)]).unwrap();
    let grid = Grid::new(Num::ONE, bounds).unwrap();
    let walker = Walker {
        layer: Layer::FIRST,
        radius: Num::ZERO,
    };
    let mut costs = CrowdCosts::new(grid, &WalkerKinds::from([walker]), []);
    let mut ids = IdAllocator::default();
    let radius = Radius::new(Num::int(6) / 5).unwrap();
    let stamp = |id, x: i64, z: i64| Stamp {
        body: IndexedBody {
            id,
            position: Vec3::new(Num::int(x) + Num::HALF, Num::ZERO, Num::int(z) + Num::HALF),
            shape: Shape::Circle(radius),
            layer: Layer::FIRST,
        },
        team: Some(Team::new(0)),
        standing: Standing::Movable,
    };
    let [one, two] = [(); 2].map(|()| ids.allocate());
    costs.update([stamp(one, 2, 2), stamp(two, 5, 1)]);
    let column_major = |held: &[bool]| {
        let mut bits = vec![0_u64; 1];
        for (cell, _) in held.iter().enumerate().filter(|&(_, &held)| held) {
            let bit = cell % 7 * 5 + cell / 7;
            bits[bit / 64] |= 1 << (bit % 64);
        }
        bits
    };
    let kind = &costs.kinds[0];
    let held: Vec<bool> = kind.standing.iter().map(|&count| count > 0).collect();
    // The first unit's plus sign and the second's, 5 cells each.
    assert_eq!(held.iter().filter(|&&held| held).count(), 10);
    assert_eq!(kind.stood, column_major(&held));
    let team = &kind.teams[0];
    let held: Vec<bool> = (0..35)
        .map(|cell| team.movable[cell] + team.held[cell] > 0)
        .collect();
    assert_eq!(team.stood, column_major(&held));
}

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
    let mut costs = CrowdCosts::new(grid, &WalkerKinds::from([walker]), []);
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
