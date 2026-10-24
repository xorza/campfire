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
        arrived: None,
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
        goal,
        depth: AsideDepth::default(),
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
            arrived: None,
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
            goal,
            depth: AsideDepth::default(),
        };
        steering
            .steer(&mut planner, &grid, &statics, steered, route.read().ahead())
            .is_some()
    });
    assert_eq!(detours, [false, true]);
}

#[test]
fn a_walker_moves_aside_only_an_idle_unit_it_may_move_on_its_next_stretch() {
    // A walker of 0.5 m of team 0 at the origin, bound for (8, 0), with a stretch of 4 m: units of
    // 0.5 m stand round it. An idle friend ahead at (0.875, 0.25), 0.25 m to its left, steps on to
    // the left until it is the two radii, 1 m, off its way, to (0.875, 1); one on the way at
    // (3, 0) steps to its left, to (3, 1). An idle friend behind it at (−0.875, 0), one past the
    // stretch and the two radii at (5, 0), an idle enemy ahead at (0.875, −0.25), and a friend
    // ahead with something under way at (0.25, −0.875) stay; a mode that moves no unit moves none.
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
            arrived: None,
        }
    };
    let bodies = [
        stands(eighths(7, 2), 0, true),
        stands(eighths(24, 0), 0, true),
        stands(eighths(-7, 0), 0, true),
        stands(eighths(40, 0), 0, true),
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
        stuck: false,
        gathering: false,
        goal: eighths(64, 0),
        depth: AsideDepth::default(),
    };
    let stretch = (eighths(64, 0), Num::int(4));
    let asides = |moves, depth| {
        let mut asides = Vec::new();
        let passage = 3..5;
        steering.asides(
            Steered { depth, ..steered },
            stretch,
            moves,
            &passage,
            &mut asides,
        );
        asides.sort_by_key(|aside| aside.id);
        asides
    };
    let aside = |at: usize, to, depth| Aside {
        id: bodies[at].body.id,
        to,
        depth,
        passage: 3..5,
        radius: half,
    };
    // A walk of its own and a step aside 1 deep move them, each aside with the walker's depth,
    // which the friend's own walk keeps ahead of the walker and takes a step deeper to a side;
    // one 2 deep moves none (04-capabilities/navigation.md, An idle friend steps aside;
    // walkers-get-through.md, A friend yields through the friends that yield).
    let [own, once, twice] =
        [0, 1, 2].map(|deep| (0..deep).fold(AsideDepth::default(), |depth, _| depth.deeper()));
    for walk in [own, once] {
        assert_eq!(
            asides(Moves::Team, walk),
            [
                aside(0, eighths(7, 8), walk),
                aside(1, eighths(24, 8), walk)
            ]
        );
    }
    assert_eq!(asides(Moves::Team, twice), []);
    assert_eq!(asides(Moves::None, own), []);
}
