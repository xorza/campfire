use campfire_geometry::{Bounds, Grid, Radius, Shape};
use campfire_sim::IdAllocator;

use super::*;
use crate::route::internals::PlannedRoute;
use crate::terrain::WallGrid;
use crate::walker_kinds::WalkerKinds;

/// The point `(x, z)` in eighths of a meter.
fn eighths(x: i64, z: i64) -> Vec3 {
    let eighth = |value: i64| Num::from_bits(value << (Num::FRAC_BITS - 3));
    Vec3::new(eighth(x), Num::ZERO, eighth(z))
}

#[test]
fn a_wide_walker_counts_every_unit_that_blocks_a_cell_of_its_window() {
    // Cells of 0.25 m, so a window reaches 8 cells, 2 m, each way. A walker of 2.5 m at
    // (0.875, 0), the center of its cell, bound for (8, 0), looks 2 m ahead, to (2.875, 0);
    // a unit of 0.25 m that stands at (5.375, 0) comes 2.5 m from there, within the two
    // radii, 2.75 m, so the walker steers, and the unit blocks the window's last centers. It
    // is a blocker, which a search of twice the window, to x = 4.875, would miss: the index's
    // buckets are 5 m, twice the walker, and the unit's box, from x = 5.125, is in the second.
    let quarter = Num::QUARTER;
    let wide = Num::from_int(10).unwrap() * quarter;
    let walker = Walker {
        layer: Layer::FIRST,
        radius: wide,
    };
    let bounds = Bounds::new(
        [Num::from_int(-8).unwrap(); 2],
        [Num::from_int(8).unwrap(); 2],
    );
    let cells = Grid::new(quarter, bounds.unwrap()).unwrap();
    let mut planner = RoutePlanner::new(&cells);
    let grid = PathingGrid::new(cells, WalkerKinds::from([walker]), &WallGrid::default());
    let statics = BodyIndex::new(wide);
    let mut ids = IdAllocator::default();
    let (id, blocker) = (ids.allocate(), ids.allocate());
    let standing = IndexedBody {
        id: blocker,
        position: eighths(43, 0),
        shape: Shape::Circle(Radius::new(quarter).unwrap()),
        layer: Layer::FIRST,
    };
    let mut steering = Steering::default();
    let still = TickBody {
        body: standing,
        team: None,
        walks: false,
        gathers: false,
        idle: true,
    };
    steering.read(&statics, [still]);
    let goal = eighths(64, 0);
    let route = PlannedRoute::new(goal, eighths(7, 0), &[goal]);
    let steered = Steered {
        id,
        position: eighths(7, 0),
        team: None,
        step: quarter,
        walker,
        stuck: false,
        gathering: false,
    };
    steering.steer(&mut planner, &grid, &statics, steered, route.read().ahead());
    assert_eq!(steering.blockers, [standing]);
}

#[test]
fn a_walker_that_gathers_steers_round_no_unit_that_gathers() {
    // A unit of a quarter meter stands at (1, 0), in the way of a walker of a quarter meter
    // at the origin bound for (6, 0), which looks 2 m ahead: a walker that gathers, past a
    // unit that gathers too, sees no blocker and keeps its route; one that does not gather
    // steers round it, by a short route to (2, 0).
    let quarter = Num::QUARTER;
    let walker = Walker {
        layer: Layer::FIRST,
        radius: quarter,
    };
    let bounds = Bounds::new([Num::int(-8); 2], [Num::int(8); 2]);
    let cells = Grid::new(quarter, bounds.unwrap()).unwrap();
    let grid = PathingGrid::new(cells, WalkerKinds::from([walker]), &WallGrid::default());
    let statics = BodyIndex::new(quarter);
    let mut ids = IdAllocator::default();
    let (id, blocker) = (ids.allocate(), ids.allocate());
    let standing = IndexedBody {
        id: blocker,
        position: eighths(8, 0),
        shape: Shape::Circle(Radius::new(quarter).unwrap()),
        layer: Layer::FIRST,
    };
    let goal = eighths(48, 0);
    let route = PlannedRoute::new(goal, eighths(0, 0), &[goal]);
    let detours = [true, false].map(|gathering| {
        let mut planner = RoutePlanner::new(&cells);
        let mut steering = Steering::default();
        let gathering_still = TickBody {
            body: standing,
            team: None,
            walks: false,
            gathers: true,
            idle: false,
        };
        steering.read(&statics, [gathering_still]);
        let steered = Steered {
            id,
            position: eighths(0, 0),
            team: None,
            step: quarter,
            walker,
            stuck: false,
            gathering,
        };
        steering
            .steer(&mut planner, &grid, &statics, steered, route.read().ahead())
            .is_some()
    });
    assert_eq!(detours, [false, true]);
}

#[test]
fn a_stuck_walker_moves_aside_only_an_idle_friend_ahead_of_it() {
    // A walker of 0.5 m of team 0 at the origin, bound for (8, 0), a quarter meter a step, and
    // four units of 0.5 m that stand within the two radii and its step, 1.25 m, of it: an idle
    // friend ahead at (0.875, 0.25), 0.25 m to its left, steps on to the left until it is the
    // two radii, 1 m, off its way, to (0.875, 1); an idle friend behind it at (−0.875, 0), an
    // idle enemy ahead at (0.875, −0.25) and a friend ahead with something under way at
    // (0.25, −0.875) stay.
    let half = Num::HALF;
    let statics = BodyIndex::new(Num::int(4));
    let mut ids = IdAllocator::default();
    let walker = ids.allocate();
    let body = |id, position| IndexedBody {
        id,
        position,
        shape: Shape::Circle(Radius::new(half).unwrap()),
        layer: Layer::FIRST,
    };
    let mut stands = |position, team, idle| {
        let id = ids.allocate();
        TickBody {
            body: body(id, position),
            team: Some(Team::new(team)),
            walks: false,
            gathers: false,
            idle,
        }
    };
    let bodies = [
        stands(eighths(7, 2), 0, true),
        stands(eighths(-7, 0), 0, true),
        stands(eighths(7, -2), 1, true),
        stands(eighths(2, -7), 0, false),
    ];
    let mut steering = Steering::default();
    steering.read(&statics, bodies);
    let steered = Steered {
        id: walker,
        position: eighths(0, 0),
        team: Some(Team::new(0)),
        step: Num::QUARTER,
        walker: Walker {
            layer: Layer::FIRST,
            radius: half,
        },
        stuck: true,
        gathering: false,
    };
    let mut asides = Vec::new();
    steering.asides(steered, eighths(64, 0), &mut asides);
    let ahead = bodies[0].body.id;
    assert_eq!(
        asides,
        [Aside {
            id: ahead,
            to: eighths(7, 8)
        }]
    );
}
